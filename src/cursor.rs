// cursor.rs
// cargador de temas xcursor para vinland
//
// el formato xcursor (.Xcursor) es un archivo binario que puede contener
// multiples imagenes (frames) de distintos tamaños, con metadatos de hotspot
// y delay para animaciones. el crate `xcursor` parsea estos archivos.
//
// flujo:
//   1. buscar el archivo del cursor en $XCURSOR_PATH o /usr/share/icons/<theme>/cursors/
//   2. parsear con xcursor::parser::parse_xcursor()
//   3. elegir el frame de tamaño mas cercano al size pedido
//   4. subir cada frame como GlesTexture via renderer.import_memory()
//
// nota: si el tema no se encuentra, se intenta con el tema "default" como fallback

use smithay::backend::allocator::Fourcc;
use smithay::backend::renderer::gles::GlesTexture;
use smithay::backend::renderer::ImportMem;
use smithay::backend::renderer::gles::GlesRenderer;
use smithay::utils::{Physical, Point, Size};

/// un frame de cursor xcursor ya subido a la GPU
pub struct CursorFrame {
    pub texture: GlesTexture,
    pub size: Size<i32, Physical>,
    /// hotspot en coordenadas fisicas (offset del punto caliente desde la esquina sup-izq)
    pub hotspot: Point<i32, Physical>,
    /// duracion del frame en ms (0 = estatico, no animar)
    pub delay_ms: u32,
}

/// un cursor cargado con todos sus frames (puede ser estatico con 1 frame o animado con varios)
pub struct LoadedCursor {
    pub frames: Vec<CursorFrame>,
}

/// intenta cargar un cursor xcursor por nombre desde el tema dado
/// si el tema falla, intenta con "default"; si también falla, retorna None
pub fn load_cursor(
    renderer: &mut GlesRenderer,
    theme: &str,
    size: u32,
    cursor_name: &str,
) -> Option<LoadedCursor> {
    // intentar cargar desde el tema pedido
    if let Some(cursor) = try_load_from_theme(renderer, theme, size, cursor_name) {
        return Some(cursor);
    }

    // fallback al tema "default"
    if theme != "default" {
        tracing::warn!(
            "[cursor] no se encontró '{}' en tema '{}', intentando con 'default'",
            cursor_name,
            theme
        );
        if let Some(cursor) = try_load_from_theme(renderer, "default", size, cursor_name) {
            return Some(cursor);
        }
    }

    tracing::warn!(
        "[cursor] no se pudo cargar el cursor '{}' (tema: '{}')",
        cursor_name,
        theme
    );
    None
}

/// busca y carga un cursor desde un tema especifico
fn try_load_from_theme(
    renderer: &mut GlesRenderer,
    theme: &str,
    size: u32,
    cursor_name: &str,
) -> Option<LoadedCursor> {
    let path = find_cursor_file(theme, cursor_name)?;

    let data = match std::fs::read(&path) {
        Ok(d) => d,
        Err(e) => {
            tracing::warn!("[cursor] error leyendo {:?}: {}", path, e);
            return None;
        }
    };

    let images = match xcursor::parser::parse_xcursor(&data) {
        Some(imgs) => imgs,
        None => {
            tracing::warn!("[cursor] error parseando xcursor {:?}", path);
            return None;
        }
    };

    if images.is_empty() {
        tracing::warn!("[cursor] xcursor vacío: {:?}", path);
        return None;
    }

    // filtrar imágenes del tamaño más cercano al pedido
    // xcursor guarda múltiples tamaños; elegir el más próximo
    let best_size = pick_best_size(&images, size);
    let frames: Vec<_> = images
        .iter()
        .filter(|img| img.size == best_size)
        .collect();

    let mut cursor_frames = Vec::new();

    for img in &frames {
        // xcursor almacena RGBA en orden R,G,B,A → subir como Abgr8888 con swap
        // el crate xcursor guarda los pixels como u32 ARGB (big endian en el archivo)
        // en memoria de la maquina (little endian) queda como BGRA
        // GlesRenderer espera Abgr8888 (RGBA en memoria) -> necesitamos swappear canales
        let width = img.width as i32;
        let height = img.height as i32;

        // convertir de BGRA (como lo da xcursor en LE) a RGBA para OpenGL
        let mut rgba = Vec::with_capacity((width * height * 4) as usize);
        for &pixel in &img.pixels_argb {
            let a = ((pixel >> 24) & 0xFF) as u8;
            let r = ((pixel >> 16) & 0xFF) as u8;
            let g = ((pixel >> 8) & 0xFF) as u8;
            let b = (pixel & 0xFF) as u8;
            // Fourcc::Abgr8888 espera [R, G, B, A] en memoria
            rgba.push(r);
            rgba.push(g);
            rgba.push(b);
            rgba.push(a);
        }

        match renderer.import_memory(
            &rgba,
            Fourcc::Abgr8888,
            (width, height).into(),
            false,
        ) {
            Ok(texture) => {
                cursor_frames.push(CursorFrame {
                    texture,
                    size: Size::from((width, height)),
                    hotspot: Point::from((img.xhot as i32, img.yhot as i32)),
                    delay_ms: img.delay,
                });
            }
            Err(e) => {
                tracing::warn!("[cursor] error importando frame a GL: {}", e);
            }
        }
    }

    if cursor_frames.is_empty() {
        return None;
    }

    tracing::info!(
        "[cursor] tema '{}', cursor '{}' cargado: {} frame(s) de {}px",
        theme,
        cursor_name,
        cursor_frames.len(),
        best_size
    );

    Some(LoadedCursor { frames: cursor_frames })
}

/// elige el tamaño de frame más cercano al solicitado de entre los disponibles
fn pick_best_size(images: &[xcursor::parser::Image], requested: u32) -> u32 {
    images
        .iter()
        .map(|img| img.size)
        .min_by_key(|&s| (s as i64 - requested as i64).unsigned_abs())
        .unwrap_or(requested)
}

/// busca el archivo del cursor en las rutas estándar:
///   1. $XCURSOR_PATH (separado por ':')
///   2. ~/.local/share/icons/<theme>/cursors/<name>
///   3. /usr/share/icons/<theme>/cursors/<name>
///   4. /usr/share/pixmaps/<name>
fn find_cursor_file(theme: &str, cursor_name: &str) -> Option<std::path::PathBuf> {
    // candidatos desde $XCURSOR_PATH
    if let Ok(xcursor_path) = std::env::var("XCURSOR_PATH") {
        for base in xcursor_path.split(':') {
            let candidate = std::path::PathBuf::from(base)
                .join(theme)
                .join("cursors")
                .join(cursor_name);
            if candidate.exists() {
                return Some(candidate);
            }
        }
    }

    // ~/.local/share/icons
    if let Some(home) = std::env::var_os("HOME") {
        let candidate = std::path::PathBuf::from(home)
            .join(".local/share/icons")
            .join(theme)
            .join("cursors")
            .join(cursor_name);
        if candidate.exists() {
            return Some(candidate);
        }
    }

    // /usr/share/icons
    let candidate = std::path::PathBuf::from("/usr/share/icons")
        .join(theme)
        .join("cursors")
        .join(cursor_name);
    if candidate.exists() {
        return Some(candidate);
    }

    // /usr/share/pixmaps (fallback para cursores sueltos)
    let candidate = std::path::PathBuf::from("/usr/share/pixmaps").join(cursor_name);
    if candidate.exists() {
        return Some(candidate);
    }

    None
}
