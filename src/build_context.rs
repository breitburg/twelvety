//! Shared, read-only state for a build (the role of `TemplateConfig` + friends
//! during a build upstream).

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use crate::data::template_data::TemplateData;
use crate::engines::RenderFn;
use crate::engines::template_engine_manager::TemplateEngineManager;
use crate::extension_map::ExtensionMap;
use crate::global_dependency_map::GlobalDependencyMap;
use crate::template_config::Config;
use crate::template_layout::TemplateLayout;
use crate::util::console_logger::ConsoleLogger;
use crate::util::exists_cache::ExistsCache;
use crate::util::project_directories::ProjectDirectories;

pub struct BuildContext {
    pub config: Arc<Config>,
    pub dirs: ProjectDirectories,
    pub extension_map: Arc<ExtensionMap>,
    pub engine_manager: Arc<TemplateEngineManager>,
    pub uses: Arc<Mutex<GlobalDependencyMap>>,
    pub exists_cache: Arc<ExistsCache>,
    pub logger: Arc<ConsoleLogger>,
    pub template_data: Arc<TemplateData>,
    pub run_mode: String,
    /// `LayoutCache`
    pub layout_cache: Mutex<HashMap<String, Arc<TemplateLayout>>>,
    /// Compile cache (`TemplateContent.getCaches().compile`)
    pub compile_cache: Mutex<HashMap<String, RenderFn>>,
    /// Input file contents cache
    pub input_cache: Mutex<HashMap<String, String>>,
    /// Warning ids already logged
    pub warned: Mutex<std::collections::HashSet<String>>,
}

impl BuildContext {
    /// Create the shared build state from an initialized `TemplateConfig`
    pub fn create(
        tc: &crate::template_config::TemplateConfig,
        logger: Arc<ConsoleLogger>,
        run_mode: &str,
        env: crate::value::Value,
    ) -> anyhow::Result<(Arc<BuildContext>, Arc<crate::file_system_search::FileSystemSearch>)> {
        use crate::value::{Map, Value};
        let config = tc.get_config();
        let dirs = tc.directories.clone();
        let mut extension_map = ExtensionMap::new(&config.extension_map);
        extension_map.set_formats(&config.template_formats);
        let extension_map = Arc::new(extension_map);
        {
            let em = extension_map.clone();
            let mut arg = Map::new();
            arg.insert(
                "removeTemplateExtension".into(),
                Value::function(move |args| {
                    Ok(Value::str(em.remove_template_extension(
                        &args.first().map(|a| a.to_output_string()).unwrap_or_default(),
                    )))
                }),
            );
            config
                .events
                .emit("buildawesome.extensionmap", &Value::from_map(arg))?;
        }
        let mut search = crate::file_system_search::FileSystemSearch::new();
        search.allow_case_collisions = config.check_error_reporting("allowCaseCollisions", "allow")
            || config.check_error_reporting("caseCollisions", "allow");
        let search = Arc::new(search);
        let engine_manager = Arc::new(TemplateEngineManager::new(
            config.clone(),
            dirs.clone(),
            tc.uses.clone(),
        ));
        let mut template_data = TemplateData::new(
            config.clone(),
            dirs.clone(),
            extension_map.clone(),
            search.clone(),
            tc.exists_cache.clone(),
            tc.uses.clone(),
            logger.clone(),
        );
        template_data.environment_variables = env;
        let template_data = Arc::new(template_data);
        config
            .events
            .emit("buildawesome.directories", &dirs.get_userspace_instance())?;
        let ctx = Arc::new(BuildContext {
            config: config.clone(),
            dirs,
            extension_map,
            engine_manager,
            uses: tc.uses.clone(),
            exists_cache: tc.exists_cache.clone(),
            logger,
            template_data,
            run_mode: run_mode.to_string(),
            layout_cache: Mutex::new(HashMap::new()),
            compile_cache: Mutex::new(HashMap::new()),
            input_cache: Mutex::new(HashMap::new()),
            warned: Mutex::new(Default::default()),
        });
        // Plugins needing engines at render time (e.g. the Render plugin) get a handle
        config.events.emit(
            "buildawesome#buildcontext",
            &Value::Dynamic(Arc::new(BuildContextRef(Arc::downgrade(&ctx)))),
        )?;
        Ok((ctx, search))
    }

    pub fn is_watch_or_serve(&self) -> bool {
        self.run_mode == "watch" || self.run_mode == "serve"
    }

    pub fn warn_once(&self, id: &str, message: &str, force: bool) {
        let mut warned = self.warned.lock().unwrap();
        if warned.insert(id.to_string()) {
            if force {
                self.logger.force_warn(message);
            } else {
                self.logger.warn(message);
            }
        }
    }

    pub fn path_prefix(&self) -> String {
        self.config.path_prefix.clone()
    }
}

/// A weak reference to the build context, passed to plugins through an internal event
pub struct BuildContextRef(pub std::sync::Weak<BuildContext>);

impl crate::value::DynObject for BuildContextRef {
    fn get(&self, _key: &str) -> Option<crate::value::Value> {
        None
    }
    fn keys(&self) -> Vec<String> {
        vec![]
    }
    fn type_name(&self) -> &'static str {
        "build-context"
    }
    fn as_any(&self) -> Option<&dyn std::any::Any> {
        Some(self)
    }
}

impl BuildContextRef {
    /// Extract the build context from an event argument
    pub fn from_value(v: &crate::value::Value) -> Option<Arc<BuildContext>> {
        match v {
            crate::value::Value::Dynamic(d) => d
                .as_any()
                .and_then(|a| a.downcast_ref::<BuildContextRef>())
                .and_then(|r| r.0.upgrade()),
            _ => None,
        }
    }
}
