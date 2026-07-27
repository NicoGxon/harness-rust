mod model;
mod setup;

pub use model::TyphonConfig;
pub use setup::{
    configurar_modelo, configurar_prompt, configurar_prompt_file, configurar_proveedor,
    configurar_razonamiento, configurar_temperatura, configurar_verbose, resolver_config,
};
