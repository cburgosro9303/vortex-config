use serde::Deserialize;

/// Extractor para rutas /{app}/{profile}
#[derive(Debug, Deserialize)]
pub struct AppProfilePath {
    pub app: String,
    pub profile: String,
}

/// Extractor para rutas /{app}/{profile}/{label}
#[derive(Debug, Deserialize)]
pub struct AppProfileLabelPath {
    pub app: String,
    pub profile: String,
    pub label: String,
}

impl AppProfilePath {
    /// Parsea el string de profiles separados por coma.
    pub fn profiles(&self) -> Vec<String> {
        self.profile
            .split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect()
    }

    /// Valida que los parametros no esten vacios.
    pub fn validate(&self) -> Result<(), String> {
        if self.app.trim().is_empty() {
            return Err("Application name cannot be empty".to_string());
        }
        if self.profile.trim().is_empty() {
            return Err("Profile cannot be empty".to_string());
        }
        Ok(())
    }
}

impl AppProfileLabelPath {
    pub fn profiles(&self) -> Vec<String> {
        self.profile
            .split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect()
    }

    /// Decodifica y sanitiza el label.
    pub fn sanitized_label(&self) -> String {
        urlencoding::decode(&self.label)
            .map(|s| s.into_owned())
            .unwrap_or_else(|_| self.label.clone())
    }

    pub fn validate(&self) -> Result<(), String> {
        if self.app.trim().is_empty() {
            return Err("Application name cannot be empty".to_string());
        }
        if self.profile.trim().is_empty() {
            return Err("Profile cannot be empty".to_string());
        }
        if self.label.trim().is_empty() {
            return Err("Label cannot be empty".to_string());
        }
        Ok(())
    }
}

// Conversion de AppProfileLabelPath a AppProfilePath
impl From<AppProfileLabelPath> for AppProfilePath {
    fn from(path: AppProfileLabelPath) -> Self {
        Self {
            app: path.app,
            profile: path.profile,
        }
    }
}
