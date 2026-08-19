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

impl Config {
    // puebla parsed_keybinds con los atajos por defecto cuando no hay [keybinds] en el config
    pub fn populate_default_keybinds(&mut self) {
        use smithay::input::keyboard::Keysym as K;

        let digits = [
            K::_1,
            K::_2,
            K::_3,
            K::_4,
            K::_5,
            K::_6,
            K::_7,
            K::_8,
            K::_9,
        ];

        for (i, &sym) in digits.iter().enumerate() {
            // Super+N -> cambiar a workspace N
            self.parsed_keybinds.push(ParsedKeybind {
                logo: true,
                shift: false,
                ctrl: false,
                alt: false,
                sym,
                action: KeyAction::Workspace(i),
            });
            // Super+Shift+N -> mover ventana activa a workspace N
            self.parsed_keybinds.push(ParsedKeybind {
                logo: true,
                shift: true,
                ctrl: false,
                alt: false,
                sym,
                action: KeyAction::MoveToWorkspace(i),
            });
        }

        // Super+Shift+Q -> cerrar ventana activa
        self.parsed_keybinds.push(ParsedKeybind {
            logo: true,
            shift: true,
            ctrl: false,
            alt: false,
            sym: K::Q,
            action: KeyAction::Close,
        });

        // Super+Shift+E -> salir del compositor
        self.parsed_keybinds.push(ParsedKeybind {
            logo: true,
            shift: true,
            ctrl: false,
            alt: false,
            sym: K::E,
            action: KeyAction::Exit,
        });

        // Super+Return -> lanzar terminal
        self.parsed_keybinds.push(ParsedKeybind {
            logo: true,
            shift: false,
            ctrl: false,
            alt: false,
            sym: K::Return,
            action: KeyAction::Exec("alacritty".to_string()),
        });
    }

    // parsea el hashmap crudo [keybinds] del toml y llena parsed_keybinds
    // si [keybinds] esta vacio, carga los atajos por defecto
    pub fn parse_keybinds_in_place(&mut self) {
        if self.keybinds.is_empty() {
            self.populate_default_keybinds();
            return;
        }
        self.parsed_keybinds.clear();
        for (key_str, action_str) in &self.keybinds {
            match (parse_key_combination(key_str), parse_action(action_str)) {
                (Some((logo, shift, ctrl, alt, sym)), Some(action)) => {
                    self.parsed_keybinds.push(ParsedKeybind {
                        logo,
                        shift,
                        ctrl,
                        alt,
                        sym,
                        action,
                    });
                }
                _ => {
                    tracing::warn!(
                        "vinland: atajo invalido en config.toml: '{}' = '{}'",
                        key_str,
                        action_str
                    );
                }
            }
        }
    }
}

// plantilla que se escribe en disco la primera vez que arranca el compositor
const DEFAULT_CONFIG_TEMPLATE: &str = r#"# ==============================================================================
# vinland - archivo de configuracion
# ruta: ~/.config/vinland/config.toml
# ==============================================================================

[background]
# [R, G, B, A]
color = [0.05, 0.05, 0.1, 1.0]

# ruta a la imagen de fondo (PNG, JPEG, WebP)
# wallpaper = "~/Pictures/wallpaper.png"

# modo de escalado: "fill" (cubre recortando), "fit" (con bandas),
# "stretch" (estira), "center" (centrado), "tile" (mosaico)
wallpaper_mode = "fill"

[tiling]
gap = 8
master_ratio = 0.55

[keyboard]
# distribucion del teclado (ej: "es", "latam", "us")
layout = "us"
# repeat_delay en ms, repeat_rate en pulsaciones/segundo
repeat_delay = 180
repeat_rate = 60

[floating]
dialog_width = 600
dialog_height = 500

[keybinds]
# formato: "MOD+TECLA" = "accion [argumentos]"
# modificadores: Super (o Mod4/Logo), Shift, Ctrl, Alt
# acciones: workspace <1-9>, move_to_workspace <1-9>, close, exit, exec <cmd>
"Super+1" = "workspace 1"
"Super+2" = "workspace 2"
"Super+3" = "workspace 3"
"Super+4" = "workspace 4"
"Super+5" = "workspace 5"
"Super+6" = "workspace 6"
"Super+7" = "workspace 7"
"Super+8" = "workspace 8"
"Super+9" = "workspace 9"

"Super+Shift+1" = "move_to_workspace 1"
"Super+Shift+2" = "move_to_workspace 2"
"Super+Shift+3" = "move_to_workspace 3"
"Super+Shift+4" = "move_to_workspace 4"
"Super+Shift+5" = "move_to_workspace 5"
"Super+Shift+6" = "move_to_workspace 6"
"Super+Shift+7" = "move_to_workspace 7"
"Super+Shift+8" = "move_to_workspace 8"
"Super+Shift+9" = "move_to_workspace 9"

"Super+Shift+q" = "close"
"Super+Return" = "exec alacritty"
"#;

// load() $XDG_CONFIG_HOME/vinland/config.toml
// si no existe lo crea con la plantilla por defecto; si falla el parseo, usa defaults
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
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            // si el archivo no existe, lo creamos con la plantilla comentada
            if let Some(parent) = path.parent() {
                let _ = std::fs::create_dir_all(parent);
            }
            match std::fs::write(&path, DEFAULT_CONFIG_TEMPLATE) {
                Ok(_) => tracing::info!("vinland: config creado en {:?}", path),
                Err(e) => tracing::warn!("vinland: no se pudo crear config: {}", e),
            }
            return Config::default();
        }
        Err(e) => {
            eprintln!("vinland: no se pudo leer {:?}: {}", path, e);
            return Config::default();
        }
    };

    match toml::from_str::<Config>(&text) {
        Ok(mut cfg) => {
            cfg.parse_keybinds_in_place();
            cfg
        }
        Err(e) => {
            eprintln!("vinland: error en config.toml: {}", e);
            Config::default()
        }
    }
}

// parsea una combinacion de teclas como "Super+Shift+1" o "Ctrl+Alt+t"
// retorna (logo, shift, ctrl, alt, keysym) o None si la combinacion es invalida
fn parse_key_combination(
    s: &str,
) -> Option<(bool, bool, bool, bool, smithay::input::keyboard::Keysym)> {
    use smithay::input::keyboard::xkb;

    let parts: Vec<&str> = s.split('+').map(|p| p.trim()).collect();
    if parts.is_empty() {
        return None;
    }

    let mut logo = false;
    let mut shift = false;
    let mut ctrl = false;
    let mut alt = false;
    let mut key_name: Option<&str> = None;

    for part in parts {
        match part.to_lowercase().as_str() {
            "super" | "mod4" | "logo" | "win" => logo = true,
            "shift" => shift = true,
            "ctrl" | "control" => ctrl = true,
            "alt" | "mod1" => alt = true,
            _ => key_name = Some(part),
        }
    }

    let key_str = key_name?;

    // nombres comunes que xkb no reconoce directamente
    let sym = match key_str.to_lowercase().as_str() {
        "return" | "enter" => smithay::input::keyboard::Keysym::Return,
        "esc" | "escape" => smithay::input::keyboard::Keysym::Escape,
        "space" => smithay::input::keyboard::Keysym::space,
        "backspace" => smithay::input::keyboard::Keysym::BackSpace,
        "tab" => smithay::input::keyboard::Keysym::Tab,
        _ => {
            // resolver por nombre xkb (insensible a mayusculas primero)
            let sym = xkb::keysym_from_name(key_str, xkb::KEYSYM_CASE_INSENSITIVE);
            if sym == smithay::input::keyboard::Keysym::new(0) {
                let sym_exact = xkb::keysym_from_name(key_str, xkb::KEYSYM_NO_FLAGS);
                if sym_exact == smithay::input::keyboard::Keysym::new(0) {
                    return None;
                }
                sym_exact
            } else {
                sym
            }
        }
    };

    Some((logo, shift, ctrl, alt, sym))
}

// parsea una accion como "workspace 1", "close", "exec alacritty"
fn parse_action(s: &str) -> Option<KeyAction> {
    let parts: Vec<&str> = s.trim().split_whitespace().collect();
    if parts.is_empty() {
        return None;
    }

    match parts[0].to_lowercase().as_str() {
        "workspace" => {
            let n = parts.get(1)?.parse::<usize>().ok()?;
            (1..=9).contains(&n).then_some(KeyAction::Workspace(n - 1))
        }
        "move_to_workspace" | "movetoworkspace" | "move_workspace" => {
            let n = parts.get(1)?.parse::<usize>().ok()?;
            (1..=9)
                .contains(&n)
                .then_some(KeyAction::MoveToWorkspace(n - 1))
        }
        "close" | "kill" => Some(KeyAction::Close),
        "exit" | "quit" => Some(KeyAction::Exit),
        "exec" | "spawn" => {
            if parts.len() > 1 {
                Some(KeyAction::Exec(parts[1..].join(" ")))
            } else {
                None
            }
        }
        _ => None,
    }
}
