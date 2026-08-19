// config.rs
// structs de configuracion de vinland, cargados desde config.toml
// cada seccion del toml ([tiling], [keyboard], [keybinds], etc.) mapea a un struct
// #[serde(default)] -> si un campo no esta en el archivo, usa default

use serde::Deserialize;
use std::collections::HashMap;
use std::path::PathBuf;

// keyaction -> acciones que puede ejecutar un atajo de teclado
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum KeyAction {
    Workspace(usize),       // indice base 0 (workspace 1 -> 0)
    MoveToWorkspace(usize), // indice base 0
    Close,
    Exit,
    Exec(String),
}

// parsedkeybind -> un atajo de teclado ya resuelto con sus modificadores y accion
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedKeybind {
    pub logo: bool,
    pub shift: bool,
    pub ctrl: bool,
    pub alt: bool,
    pub sym: smithay::input::keyboard::Keysym,
    pub action: KeyAction,
}

#[derive(Debug, Deserialize)]
#[serde(default)]
pub struct Config {
    pub tiling: TilingConfig,
    pub keyboard: KeyboardConfig,
    pub background: BackgroundConfig,
    pub floating: FloatingConfig,
    // tabla [keybinds] del toml: "Super+1" = "workspace 1"
    #[serde(default)]
    pub keybinds: HashMap<String, String>,
    // atajos ya parseados, se llenan despues de cargar el toml
    #[serde(skip)]
    pub parsed_keybinds: Vec<ParsedKeybind>,
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
    pub layout: String,          // ej: "es", "us", "latam"
    pub options: Option<String>, // ej: "caps:escape"
    pub repeat_delay: i32,       // ms hasta empezar a repetir
    pub repeat_rate: i32,        // pulsaciones/segundo al mantener
}

/// Cómo escalar el wallpaper al tamaño del output
#[derive(Debug, Deserialize, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ScaleMode {
    /// Escala manteniendo aspect ratio, sin recortar (puede dejar bandas)
    Fit,
    /// Escala manteniendo aspect ratio, recortando lo que sobra
    Fill,
    /// Estira la imagen para que ocupe exactamente el output (puede deformar)
    Stretch,
    /// Centra sin escalar
    Center,
    /// Repite la imagen como mosaico
    Tile,
}

impl Default for ScaleMode {
    fn default() -> Self {
        ScaleMode::Fill
    }
}

#[derive(Debug, Deserialize)]
#[serde(default)]
pub struct BackgroundConfig {
    /// color de fondo default
    pub color: [f32; 4],
    /// ruta a la imagen de fondo (PNG, JPEG, WebP, etc.)
    pub wallpaper: Option<PathBuf>,
    /// Cómo escalar el wallpaper
    pub wallpaper_mode: ScaleMode,
}

#[derive(Debug, Deserialize)]
#[serde(default)]
pub struct FloatingConfig {
    pub dialog_width: i32,
    pub dialog_height: i32,
}

// defaults = valores que antes estaban hardcodeados en el codigo
impl Default for Config {
    fn default() -> Self {
        Self {
            tiling: TilingConfig::default(),
            keyboard: KeyboardConfig::default(),
            background: BackgroundConfig::default(),
            floating: FloatingConfig::default(),
            keybinds: HashMap::new(),
            parsed_keybinds: Vec::new(),
        }
    }
}
impl Default for TilingConfig {
    fn default() -> Self {
        Self {
            gap: 8,
            master_ratio: 0.5,
        }
    }
}
impl Default for KeyboardConfig {
    fn default() -> Self {
        Self {
            layout: String::new(),
            options: None,
            repeat_delay: 200,
            repeat_rate: 25,
        }
    }
}
impl Default for BackgroundConfig {
    fn default() -> Self {
        Self {
            color: [0.05, 0.05, 0.1, 1.0], // azul oscuro por defecto
            wallpaper: None,
            wallpaper_mode: ScaleMode::Fill,
        }
    }
}
impl Default for FloatingConfig {
    fn default() -> Self {
        Self {
            dialog_width: 600,
            dialog_height: 500,
        }
    }
}

// load() $XDG_CONFIG_HOME/vinland/config.toml
// si no existe o falla el parseo, devuelve Config::default() sin crashear
pub fn load() -> Config {
    let path = {
        let base = std::env::var_os("XDG_CONFIG_HOME")
            .map(std::path::PathBuf::from)
            .unwrap_or_else(|| {
                std::env::var_os("HOME")
                    .map(std::path::PathBuf::from)
                    .unwrap_or_else(|| std::path::PathBuf::from("."))
                    .join(".config")
            });
        base.join("vinland").join("config.toml")
    };

    let text = match std::fs::read_to_string(&path) {
        Ok(t) => t,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Config::default(),
        Err(e) => {
            eprintln!("vinland: no se pudo leer {:?}: {}", path, e);
            return Config::default();
        }
    };

    match toml::from_str::<Config>(&text) {
        Ok(cfg) => cfg,
        Err(e) => {
            eprintln!("vinland: error en config.toml: {}", e);
            Config::default()
        }
    }
}
