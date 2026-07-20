// config.rs
// structs de configuración de vinland, cargados desde config.toml
// cada sección del TOML ([tiling], [keyboard], etc.) mapea a un struct
// #[serde(default)] -> si un campo no está en el archivo, usa Default

use serde::Deserialize;

#[derive(Debug, Deserialize)]
#[serde(default)]
pub struct Config {
    pub tiling:     TilingConfig,
    pub keyboard:   KeyboardConfig,
    pub background: BackgroundConfig,
    pub floating:   FloatingConfig,
}

#[derive(Debug, Deserialize)]
#[serde(default)]
pub struct TilingConfig {
    pub gap: i32,
    pub master_ratio: f32, // fracción de pantalla que ocupa el master (0.0–1.0)
}

#[derive(Debug, Deserialize)]
#[serde(default)]
pub struct KeyboardConfig {
    pub layout:       String,         // ej: "es", "us", "latam"
    pub options:      Option<String>, // ej: "caps:escape"
    pub repeat_delay: i32,            // ms hasta empezar a repetir
    pub repeat_rate:  i32,            // pulsaciones/segundo al mantener
}

#[derive(Debug, Deserialize)]
#[serde(default)]
pub struct BackgroundConfig {
    pub color: [f32; 4], // RGBA, 0.0–1.0
}

#[derive(Debug, Deserialize)]
#[serde(default)]
pub struct FloatingConfig {
    pub dialog_width:  i32,
    pub dialog_height: i32,
}

// defaults = valores que antes estaban hardcodeados en el código
impl Default for Config {
    fn default() -> Self {
        Self {
            tiling:     TilingConfig::default(),
            keyboard:   KeyboardConfig::default(),
            background: BackgroundConfig::default(),
            floating:   FloatingConfig::default(),
        }
    }
}
impl Default for TilingConfig {
    fn default() -> Self { Self { gap: 8, master_ratio: 0.5 } }
}
impl Default for KeyboardConfig {
    fn default() -> Self {
        Self { layout: String::new(), options: None, repeat_delay: 200, repeat_rate: 25 }
    }
}
impl Default for BackgroundConfig {
    fn default() -> Self { Self { color: [0.0, 0.0, 0.0, 1.0] } }
}
impl Default for FloatingConfig {
    fn default() -> Self { Self { dialog_width: 600, dialog_height: 500 } }
}
