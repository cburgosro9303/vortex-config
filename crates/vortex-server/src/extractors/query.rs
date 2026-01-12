use serde::Deserialize;

/// Query parameters opcionales para endpoints de configuracion.
#[derive(Debug, Deserialize, Default)]
#[serde(default)]
pub struct ConfigQuery {
    /// Si true y el label no existe, usa el label por defecto (main/master).
    #[serde(rename = "useDefaultLabel")]
    pub use_default_label: bool,

    /// Forzar refresh del cache (bypass).
    #[serde(rename = "forceRefresh")]
    pub force_refresh: bool,
}
