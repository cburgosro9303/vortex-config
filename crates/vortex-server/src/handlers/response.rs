use serde::Serialize;
use std::collections::HashMap;

/// Response compatible con Spring Cloud Config Server.
///
/// Este struct mapea exactamente al formato JSON que retorna
/// Spring Cloud Config para mantener compatibilidad.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConfigResponse {
    /// Nombre de la aplicacion
    pub name: String,

    /// Lista de profiles activos
    pub profiles: Vec<String>,

    /// Label (branch/tag) usado, null si no se especifico
    pub label: Option<String>,

    /// Version del commit (para Git backend)
    pub version: Option<String>,

    /// Estado adicional del config server
    pub state: Option<String>,

    /// Lista de property sources en orden de precedencia
    pub property_sources: Vec<PropertySourceResponse>,
}

/// Representa un archivo de configuracion individual.
#[derive(Debug, Clone, Serialize)]
pub struct PropertySourceResponse {
    /// Nombre/path del archivo de configuracion
    pub name: String,

    /// Propiedades como mapa clave-valor
    pub source: HashMap<String, serde_json::Value>,
}

impl ConfigResponse {
    /// Crea una respuesta vacia para una aplicacion y profiles.
    pub fn empty(name: impl Into<String>, profiles: Vec<String>) -> Self {
        Self {
            name: name.into(),
            profiles,
            label: None,
            version: None,
            state: None,
            property_sources: Vec::new(),
        }
    }
}
