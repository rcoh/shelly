use anyhow::Result;
use deno_ast::{MediaType, ParseParams};
use deno_core::{
    JsRuntime, ModuleLoadResponse, ModuleLoader, ModuleSource, ModuleSourceCode, ModuleSpecifier,
    ModuleType, ResolutionKind, RuntimeOptions,
};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::rc::Rc;

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
}

pub struct HandlerRuntime {
    runtime: JsRuntime,
    handler_name: String,
}

impl HandlerRuntime {
    pub fn new() -> Result<Self> {
        let runtime = JsRuntime::new(RuntimeOptions {
            module_loader: Some(Rc::new(TsModuleLoader)),
            ..Default::default()
        });
        Ok(Self { 
            runtime,
            handler_name: String::new(),
        })
    }

    /// Load a handler from a file
    pub async fn load_handler(&mut self, path: &str) -> Result<()> {
        let resolved = std::fs::canonicalize(path)?;
        let specifier = ModuleSpecifier::from_file_path(&resolved)
            .map_err(|_| anyhow::anyhow!("Invalid path"))?;

        // Create a wrapper module that imports and exposes the handler
        let wrapper_code = format!(
            r#"
            import {{ cargoHandler }} from "{}";
            globalThis.cargoHandler = cargoHandler;
            "#,
            specifier
        );
        
        let wrapper_spec = ModuleSpecifier::parse("file:///wrapper.js")?;
        let module_id = self.runtime.load_side_es_module_from_code(&wrapper_spec, wrapper_code).await?;
        let result = self.runtime.mod_evaluate(module_id);
        self.runtime.run_event_loop(Default::default()).await?;
        result.await?;

        self.handler_name = "cargoHandler".to_string();
        Ok(())
    }

    /// Check if handler matches a command
    pub async fn matches(&mut self, command: &str) -> Result<bool> {
        let code = format!("cargoHandler.matches({})", serde_json::to_string(command)?);
        let result = self.runtime.execute_script("<matches>", code)?;
        let scope = &mut self.runtime.handle_scope();
        let local = deno_core::v8::Local::new(scope, result);
        Ok(local.is_true())
    }

    /// Create a handler instance
    pub async fn create_handler(
        &mut self,
        command: &str,
        settings: &HashMap<String, serde_json::Value>,
    ) -> Result<()> {
        let code = format!(
            "globalThis.__handler = cargoHandler.create({}, {})",
            serde_json::to_string(command)?,
            serde_json::to_string(settings)?
        );
        self.runtime.execute_script("<create>", code)?;
        Ok(())
    }

    /// Call prepare on the handler
    pub async fn prepare(&mut self) -> Result<PrepareResult> {
        let code = "JSON.stringify(globalThis.__handler.prepare())";
        let result = self.runtime.execute_script("<prepare>", code)?;
        let scope = &mut self.runtime.handle_scope();
        let local = deno_core::v8::Local::new(scope, result);
        let json_str = local.to_rust_string_lossy(scope);
        Ok(serde_json::from_str(&json_str)?)
    }

    /// Call summarize on the handler
    pub async fn summarize(
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
        let result = self.runtime.execute_script("<summarize>", code)?;
        let scope = &mut self.runtime.handle_scope();
        let local = deno_core::v8::Local::new(scope, result);
        let json_str = local.to_rust_string_lossy(scope);
        Ok(serde_json::from_str(&json_str)?)
    }
}
