use anyhow::Result;
use deno_ast::{MediaType, ParseParams};
use deno_core::{
    JsRuntime, ModuleLoadResponse, ModuleLoader, ModuleSource, ModuleSourceCode, ModuleSpecifier,
    ModuleType, ResolutionKind, RuntimeOptions,
};
use serde::{Deserialize, Serialize};
use std::rc::Rc;
use std::{collections::HashMap, iter::Sum};
use tokio::sync::{mpsc, oneshot};

struct TsModuleLoader;

impl ModuleLoader for TsModuleLoader {
    fn resolve(
        &self,
        specifier: &str,
        referrer: &str,
        _kind: ResolutionKind,
    ) -> Result<ModuleSpecifier, deno_core::error::AnyError> {
        deno_core::resolve_import(specifier, referrer).map_err(|e| e.into())
    }

    fn load(
        &self,
        module_specifier: &ModuleSpecifier,
        _maybe_referrer: Option<&ModuleSpecifier>,
        _is_dyn_import: bool,
        _requested_module_type: deno_core::RequestedModuleType,
    ) -> ModuleLoadResponse {
        let module_specifier = module_specifier.clone();
        let module_load = move || {
            let path = module_specifier.to_file_path().unwrap();
            let media_type = MediaType::from_path(&path);
            let (module_type, should_transpile) = match media_type {
                MediaType::JavaScript | MediaType::Mjs | MediaType::Cjs => {
                    (ModuleType::JavaScript, false)
                }
                MediaType::Jsx => (ModuleType::JavaScript, true),
                MediaType::TypeScript | MediaType::Mts | MediaType::Cts | MediaType::Tsx => {
                    (ModuleType::JavaScript, true)
                }
                MediaType::Json => (ModuleType::Json, false),
                _ => panic!("Unknown extension {:?}", path.extension()),
            };

            let code = std::fs::read_to_string(&path)?;
            let code = if should_transpile {
                let parsed = deno_ast::parse_module(ParseParams {
                    specifier: module_specifier.clone(),
                    text: code.into(),
                    media_type,
                    capture_tokens: false,
                    scope_analysis: false,
                    maybe_syntax: None,
                })?;
                parsed
                    .transpile(
                        &Default::default(),
                        &Default::default(),
                        &Default::default(),
                    )?
                    .into_source()
                    .text
            } else {
                code
            };

            let module = ModuleSource::new(
                module_type,
                ModuleSourceCode::Bytes(code.into_bytes().into_boxed_slice().into()),
                &module_specifier,
                None,
            );
            Ok(module)
        };

        ModuleLoadResponse::Sync(module_load())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PrepareResult {
    pub command: String,
    pub env: HashMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SummaryResult {
    pub summary: Option<String>,
    pub truncation: Option<TruncationInfo>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TruncationInfo {
    pub truncated: bool,
    pub reason: Option<String>,
    pub description: Option<String>,
}

enum RuntimeRequest {
    LoadHandler {
        path: String,
        response: oneshot::Sender<Result<()>>,
    },
    Matches {
        command: String,
        response: oneshot::Sender<Result<bool>>,
    },
    CreateHandler {
        command: String,
        settings: HashMap<String, serde_json::Value>,
        response: oneshot::Sender<Result<()>>,
    },
    Prepare {
        response: oneshot::Sender<Result<PrepareResult>>,
    },
    Summarize {
        stdout: String,
        stderr: String,
        exit_code: Option<i32>,
        response: oneshot::Sender<Result<SummaryResult>>,
    },
}

struct HandlerRuntimeInner {
    js_runtime: JsRuntime,
}

impl HandlerRuntimeInner {
    fn new() -> Self {
        let js_runtime = JsRuntime::new(RuntimeOptions {
            module_loader: Some(Rc::new(TsModuleLoader)),
            ..Default::default()
        });
        Self { js_runtime }
    }

    async fn load_handler(&mut self, path: &str) -> Result<()> {
        let resolved = std::fs::canonicalize(path)?;
        let specifier = ModuleSpecifier::from_file_path(&resolved)
            .map_err(|_| anyhow::anyhow!("Invalid path"))?;

        let wrapper_code = format!(
            r#"
            import {{ cargoHandler }} from "{}";
            globalThis.cargoHandler = cargoHandler;
            "#,
            specifier
        );

        let wrapper_spec = ModuleSpecifier::parse("file:///wrapper.js")?;
        let module_id = self
            .js_runtime
            .load_side_es_module_from_code(&wrapper_spec, wrapper_code)
            .await?;
        let result = self.js_runtime.mod_evaluate(module_id);
        self.js_runtime.run_event_loop(Default::default()).await?;
        result.await?;

        Ok(())
    }

    fn matches(&mut self, command: &str) -> Result<bool> {
        let code = format!("cargoHandler.matches({})", serde_json::to_string(command)?);
        let result = self.js_runtime.execute_script("<matches>", code)?;
        let scope = &mut self.js_runtime.handle_scope();
        let local = deno_core::v8::Local::new(scope, result);
        Ok(local.is_true())
    }

    fn create_handler(
        &mut self,
        command: &str,
        settings: &HashMap<String, serde_json::Value>,
    ) -> Result<()> {
        let code = format!(
            "globalThis.__handler = cargoHandler.create({}, {})",
            serde_json::to_string(command)?,
            serde_json::to_string(settings)?
        );
        self.js_runtime.execute_script("<create>", code)?;
        Ok(())
    }

    fn prepare(&mut self) -> Result<PrepareResult> {
        let code = "JSON.stringify(globalThis.__handler.prepare())";
        let result = self.js_runtime.execute_script("<prepare>", code)?;
        let scope = &mut self.js_runtime.handle_scope();
        let local = deno_core::v8::Local::new(scope, result);
        let json_str = local.to_rust_string_lossy(scope);
        Ok(serde_json::from_str(&json_str)?)
    }

    fn summarize(
        &mut self,
        stdout: &str,
        stderr: &str,
        exit_code: Option<i32>,
    ) -> Result<SummaryResult> {
        let code = format!(
            "JSON.stringify(globalThis.__handler.summarize({}, {}, {}))",
            serde_json::to_string(stdout)?,
            serde_json::to_string(stderr)?,
            exit_code
                .map(|c| c.to_string())
                .unwrap_or_else(|| "null".to_string())
        );
        let result = self.js_runtime.execute_script("<summarize>", code)?;
        let scope = &mut self.js_runtime.handle_scope();
        let local = deno_core::v8::Local::new(scope, result);
        let json_str = local.to_rust_string_lossy(scope);
        Ok(serde_json::from_str(&json_str)?)
    }

    async fn run(mut self, mut rx: mpsc::UnboundedReceiver<RuntimeRequest>) {
        while let Some(req) = rx.recv().await {
            match req {
                RuntimeRequest::LoadHandler { path, response } => {
                    let result = self.load_handler(&path).await;
                    let _ = response.send(result);
                }
                RuntimeRequest::Matches { command, response } => {
                    let result = self.matches(&command);
                    let _ = response.send(result);
                }
                RuntimeRequest::CreateHandler {
                    command,
                    settings,
                    response,
                } => {
                    let result = self.create_handler(&command, &settings);
                    let _ = response.send(result);
                }
                RuntimeRequest::Prepare { response } => {
                    let result = self.prepare();
                    let _ = response.send(result);
                }
                RuntimeRequest::Summarize {
                    stdout,
                    stderr,
                    exit_code,
                    response,
                } => {
                    let result = self.summarize(&stdout, &stderr, exit_code);
                    let _ = response.send(result);
                }
            }
        }
    }
}

pub struct HandlerRuntime {
    tx: mpsc::UnboundedSender<RuntimeRequest>,
}

pub async fn process(
    stdout: &str,
    stderr: &str,
    handler: &Option<HandlerRuntime>,
) -> Result<SummaryResult> {
    if let Some(handler) = handler {
        handler.summarize(stdout, stderr, None).await
    } else {
        Ok(SummaryResult {
            summary: Some(format!("{stdout}{stderr}")),
            truncation: None,
        })
    }
}

impl HandlerRuntime {
    pub fn new() -> Result<Self> {
        let (tx, rx) = mpsc::unbounded_channel();

        std::thread::spawn(move || {
            let rt = tokio::runtime::Runtime::new().unwrap();
            let inner = HandlerRuntimeInner::new();
            rt.block_on(inner.run(rx));
        });

        Ok(Self { tx })
    }

    pub async fn load_handler(&mut self, path: &str) -> Result<()> {
        let (tx, rx) = oneshot::channel();
        self.tx.send(RuntimeRequest::LoadHandler {
            path: path.to_string(),
            response: tx,
        })?;
        rx.await?
    }

    pub async fn matches(&mut self, command: &str) -> Result<bool> {
        let (tx, rx) = oneshot::channel();
        self.tx.send(RuntimeRequest::Matches {
            command: command.to_string(),
            response: tx,
        })?;
        rx.await?
    }

    pub async fn create_handler(
        &mut self,
        command: &str,
        settings: &HashMap<String, serde_json::Value>,
    ) -> Result<()> {
        let (tx, rx) = oneshot::channel();
        self.tx.send(RuntimeRequest::CreateHandler {
            command: command.to_string(),
            settings: settings.clone(),
            response: tx,
        })?;
        rx.await?
    }

    pub async fn prepare(&mut self) -> Result<PrepareResult> {
        let (tx, rx) = oneshot::channel();
        self.tx.send(RuntimeRequest::Prepare { response: tx })?;
        rx.await?
    }

    pub async fn summarize(
        &self,
        stdout: &str,
        stderr: &str,
        exit_code: Option<i32>,
    ) -> Result<SummaryResult> {
        let (tx, rx) = oneshot::channel();
        self.tx.send(RuntimeRequest::Summarize {
            stdout: stdout.to_string(),
            stderr: stderr.to_string(),
            exit_code,
            response: tx,
        })?;
        rx.await?
    }
}
