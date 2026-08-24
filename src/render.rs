/// render.rs
///
/// maneja el dibujado de cada cuadro (frame) del compositor en la pantalla.
/// entries:    el estado global `Vinland`, el cual contiene la lista de ventanas activas (buffers
///            de Wayland de los clientes), la posición del cursor lógico y el backend de winit/OpenGL.
///
/// exits:    dibuja los elementos en el framebuffer de OpenGLES via GlesRenderer
///           y envía eventos de sincronización (frame callbacks)
///           a cada cliente para permitirles actualizar y enviar el siguiente cuadro.
/// ===================================================================================================
use std::time::Instant;

use smithay::backend::renderer::element::surface::{
    render_elements_from_surface_tree, WaylandSurfaceRenderElement,
};
use smithay::backend::renderer::element::{Element, Kind, RenderElement};
use smithay::backend::renderer::gles::GlesRenderer;
use smithay::backend::renderer::Frame;
use smithay::backend::renderer::Renderer;
use smithay::backend::renderer::Texture;
use smithay::backend::renderer::Color32F;
use smithay::desktop::utils::send_frames_surface_tree;
use smithay::desktop::PopupManager;
use smithay::input::pointer::{CursorImageStatus, CursorImageSurfaceData};
use smithay::utils::IsAlive;
use smithay::utils::{Rectangle, Scale, Transform};

use crate::config::ScaleMode;
use crate::handlers::layer_shell::layer_surface_geometry;
use crate::state::Vinland;
use smithay::utils::Size;
use smithay::wayland::shell::wlr_layer::Layer;
use tracing::info;

// render_frame -> dibuja un frame completo y avisa a los clientes
//   1. colecta elementos de cada superficie wayland
//   2. limpia el fondo
//   3. dibuja cada elemento
//   4. envía el frame a la pantalla
//   5. manda frame callbacks a los clientes
// TODO: cuando haga composicion realmente (tiling) hay que pensar en los damage rects
pub fn render_frame(state: &mut Vinland, start_time: Instant) {
    let size = state.backend.window_size();
    let damage = smithay::utils::Rectangle::new((0, 0).into(), size);
    let scale = Scale::from(state.backend.scale_factor());

    // ocultar el cursor del host (Winit) si estamos dibujando un cursor personalizado
    let cursor_visible = !matches!(state.cursor_status, CursorImageStatus::Surface(_));
    state.backend.window().set_cursor_visible(cursor_visible);

    // bind() -> prepara el renderer y obtiene el framebuffer del frame actual
    // nota: bind() toma borrow mutable de state.backend, por lo tanto no podemos
    // llamar state.windows() después (ambos borran &state). recolectamos los datos
    // que necesitamos de las ventanas antes del bind, como snapshots simples.
    let window_snap: Vec<(smithay::wayland::shell::xdg::ToplevelSurface, smithay::utils::Rectangle<i32, smithay::utils::Logical>)> = state
        .windows()
        .iter()
        .filter(|w| !w.minimized && w.rect.size.w > 0 && w.rect.size.h > 0)
        .map(|w| (w.surface.clone(), w.rect))
        .collect();

    // obtener la superficie que tiene el foco de teclado para colorear el borde activo
    let focused_surface_id: Option<smithay::reexports::wayland_server::protocol::wl_surface::WlSurface> = state
        .seat
        .get_keyboard()
        .and_then(|k| k.current_focus());

    let (renderer, mut framebuffer) = state.backend.bind().unwrap();

    // 1. colectar elementos de las ventanas en su posición de tiling
    // all_elements está en orden FRONT-TO-BACK: lo que está más al frente va primero.
    // Al dibujar en reverso, lo más atrás se pinta primero y lo más adelante encima.
    //
    // Capas (de frente a atras):
    //   [cursor] → [popups xdg] → [ventanas + sus subsurfaces]
    //
    // render_elements_from_surface_tree devuelve la ventana con sus subsurfaces ya
    // ordenadas en front-to-back internamente (above_sub, toplevel, below_sub).

    let mut window_elements: Vec<WaylandSurfaceRenderElement<GlesRenderer>> = Vec::new();
    let mut popup_elements: Vec<WaylandSurfaceRenderElement<GlesRenderer>> = Vec::new();

    for (surface, rect) in &window_snap {
        // (el snapshot ya filtró minimizadas y con rect 0)
        let pos = rect.loc.to_physical_precise_round(scale);
        let elems: Vec<WaylandSurfaceRenderElement<GlesRenderer>> =
            render_elements_from_surface_tree(
                renderer,
                surface.wl_surface(),
                pos,
                scale,
                1.0,
                Kind::Unspecified,
            );
        window_elements.extend(elems);

        // 1b. popups xdg de esta ventana (PopupManager)
        for (popup, popup_location) in PopupManager::popups_for_surface(surface.wl_surface()) {
            let popup_geo_loc = popup.geometry().loc;
            let popup_loc = rect.loc + popup_location - popup_geo_loc;
            let popup_pos = popup_loc.to_physical_precise_round(scale);
            let popup_elems: Vec<WaylandSurfaceRenderElement<GlesRenderer>> =
                render_elements_from_surface_tree(
                    renderer,
                    popup.wl_surface(),
                    popup_pos,
                    scale,
                    1.0,
                    Kind::Unspecified,
                );
            popup_elements.extend(popup_elems);
        }
    }

    // 1c. Layer surfaces (Background, Bottom, Top, Overlay)
    let output_size = state
        .output
        .current_mode()
        .map(|m| m.size.to_logical(1))
        .unwrap_or_else(|| Size::from((1920, 1080)));

    let mut overlay_elements = Vec::new();
    let mut top_elements = Vec::new();
    let mut bottom_elements = Vec::new();
    let mut background_elements = Vec::new();

    for item in &state.layer_surfaces {
        if !item.surface.alive() {
            continue;
        }
        if let Some(rect) = layer_surface_geometry(&item.surface, output_size) {
            let pos = rect.loc.to_physical_precise_round(scale);
            let elems: Vec<WaylandSurfaceRenderElement<GlesRenderer>> =
                render_elements_from_surface_tree(
                    renderer,
                    item.surface.wl_surface(),
                    pos,
                    scale,
                    1.0,
                    Kind::Unspecified,
                );
            match item.layer {
                Layer::Overlay => overlay_elements.extend(elems),
                Layer::Top => top_elements.extend(elems),
                Layer::Bottom => bottom_elements.extend(elems),
                Layer::Background => background_elements.extend(elems),
            }
        }
    }

    // Construir all_elements en front-to-back:
    // [cursor] -> [Overlay] -> [Popups] -> [Top] -> [Ventanas] -> [Bottom] -> [Background]
    let mut all_elements: Vec<WaylandSurfaceRenderElement<GlesRenderer>> = Vec::new();
    all_elements.extend(overlay_elements);
    all_elements.extend(popup_elements);
    all_elements.extend(top_elements);
    all_elements.extend(window_elements);
    all_elements.extend(bottom_elements);
    all_elements.extend(background_elements);

    // 2. lógica del cursor
    // reset del cursor si la superficie ya no está viva
    let mut reset = false;
    if let CursorImageStatus::Surface(ref surface) = state.cursor_status {
        reset = !surface.alive();
    }
    if reset {
        state.cursor_status = CursorImageStatus::default_named();
    }

    // si el cursor es una superficie (dibujada por la app), la agregamos a los elementos a dibujar
    if let CursorImageStatus::Surface(ref surface) = state.cursor_status {
        // with_states accede a los metadatos asociados a la superficie del cursor
        let hotspot = smithay::wayland::compositor::with_states(surface, |states| {
            states
                .data_map
                .get::<CursorImageSurfaceData>()
                .unwrap()
                .lock()
                .unwrap()
                .hotspot
        });

        // resta en f64, aplica la escala y finalmente redondea a enteros
        let cursor_pos = (state.pointer_pos - hotspot.to_f64())
            .to_physical(scale)
            .to_i32_round();

        // let cursor_pos = cursor_pos + Point::from((100, 100)); // Cursor

        // render_elements_from_surface_tree importa el buffer del cursor
        // El cursor siempre va al FRENTE de todo (índice 0 = frente en orden front-to-back).
        // render_elements_from_surface_tree devuelve elementos en front-to-back:
        // el primer elemento es el más adelante (sobre todo lo demás).
        // Por eso el cursor va primero en la Vec, y dibujamos la Vec en reverso (back-to-front).
        let mut cursor_elems: Vec<WaylandSurfaceRenderElement<GlesRenderer>> =
            render_elements_from_surface_tree(
                renderer,
                surface,
                cursor_pos,
                scale,
                1.0,
                Kind::Cursor,
            );
        cursor_elems.append(&mut all_elements);
        all_elements = cursor_elems;
    }

    // 3. renderizado de OpenGL
    let mut frame = renderer
        .render(&mut framebuffer, size, Transform::Flipped180)
        .unwrap();

    let bg = state.config.background.color;
    frame
        .clear(smithay::backend::renderer::Color32F::from(bg), &[damage])
        .unwrap();

    // Wallpaper: dibujar la textura GL sobre el fondo antes que cualquier ventana
    if let Some(ref tex) = state.wallpaper_texture {
        let tex_size = tex.size(); // Size<i32, BufferCoord>
        let out_w = size.w as f32;
        let out_h = size.h as f32;
        let img_w = tex_size.w as f32;
        let img_h = tex_size.h as f32;

        let (src, dest) = match state.config.background.wallpaper_mode {
            ScaleMode::Stretch => (
                Rectangle::new((0.0, 0.0).into(), (img_w as f64, img_h as f64).into()),
                Rectangle::new((0, 0).into(), (size.w, size.h).into()),
            ),
            ScaleMode::Fill => {
                let out_ratio = out_w / out_h;
                let img_ratio = img_w / img_h;
                let (crop_w, crop_h) = if img_ratio > out_ratio {
                    // si la imagen es mas ancha que la pantalla se recortan los lados
                    (img_h * out_ratio, img_h)
                } else {
                    // si la imagen es mas alta que la pantalla se recortan arriba y abajo
                    (img_w, img_w / out_ratio)
                };
                let crop_x = (img_w - crop_w) / 2.0;
                let crop_y = (img_h - crop_h) / 2.0;
                (
                    Rectangle::new(
                        (crop_x as f64, crop_y as f64).into(),
                        (crop_w as f64, crop_h as f64).into(),
                    ),
                    Rectangle::new((0, 0).into(), (size.w, size.h).into()),
                )
            }
            ScaleMode::Fit => {
                let out_ratio = out_w / out_h;
                let img_ratio = img_w / img_h;
                let (fit_w, fit_h) = if img_ratio > out_ratio {
                    // La imagen es más ancha -> franjas arriba y abajo
                    (out_w, out_w / img_ratio)
                } else {
                    // La imagen es más alta -> franjas a los lados
                    (out_h * img_ratio, out_h)
                };
                let ox = ((out_w - fit_w) / 2.0) as i32;
                let oy = ((out_h - fit_h) / 2.0) as i32;
                (
                    Rectangle::new((0.0, 0.0).into(), (img_w as f64, img_h as f64).into()),
                    Rectangle::new((ox, oy).into(), (fit_w as i32, fit_h as i32).into()),
                )
            }
            ScaleMode::Center | ScaleMode::Tile => {
                if img_w >= out_w && img_h >= out_h {
                    let crop_x = (img_w - out_w) / 2.0;
                    let crop_y = (img_h - out_h) / 2.0;
                    (
                        Rectangle::new(
                            (crop_x as f64, crop_y as f64).into(),
                            (out_w as f64, out_h as f64).into(),
                        ),
                        Rectangle::new((0, 0).into(), (size.w, size.h).into()),
                    )
                } else {
                    let ox = ((out_w - img_w) / 2.0) as i32;
                    let oy = ((out_h - img_h) / 2.0) as i32;
                    (
                        Rectangle::new((0.0, 0.0).into(), (img_w as f64, img_h as f64).into()),
                        Rectangle::new((ox, oy).into(), (img_w as i32, img_h as i32).into()),
                    )
                }
            }
        };

        if let Err(e) = frame.render_texture_from_to(
            tex,
            src,
            dest,
            &[damage],
            &[], // sin opaque regions (transparencia permitida)
            Transform::Normal,
            1.0,
            None,
            &[],
        ) {
            info!("[wallpaper] error al dibujar textura: {:?}", e);
        }
    }

    // bordes SSD: dibujar los 4 segmentos de borde de cada ventana visible
    // se dibujan DESPUES del wallpaper y ANTES de las superficies de los clientes
    // para que el borde quede por detras del contenido de la ventana (no encima)
    let bw = state.config.decoration.border_width;
    if bw > 0 {
        let active_color = Color32F::from(state.config.decoration.active_border_color);
        let inactive_color = Color32F::from(state.config.decoration.inactive_border_color);

        for (surface, rect) in &window_snap {
            // determinar si esta ventana tiene el foco de teclado
            let is_active = focused_surface_id
                .as_ref()
                .map(|fs| fs == surface.wl_surface())
                .unwrap_or(false);
            let border_color = if is_active { active_color } else { inactive_color };

            // convertir el rect logico a fisico para draw_solid (que trabaja en coordenadas fisicas)
            let bw_phys = (bw as f64 * scale.x) as i32;
            let x = (rect.loc.x as f64 * scale.x) as i32;
            let y = (rect.loc.y as f64 * scale.y) as i32;
            let w_phys = (rect.size.w as f64 * scale.x) as i32;
            let h_phys = (rect.size.h as f64 * scale.y) as i32;

            // borde superior: x-bw, y-bw, ancho+2*bw, alto=bw
            let top = Rectangle::new(
                (x - bw_phys, y - bw_phys).into(),
                (w_phys + bw_phys * 2, bw_phys).into(),
            );
            // borde inferior: x-bw, y+h, ancho+2*bw, alto=bw
            let bottom = Rectangle::new(
                (x - bw_phys, y + h_phys).into(),
                (w_phys + bw_phys * 2, bw_phys).into(),
            );
            // borde izquierdo: x-bw, y, ancho=bw, alto=h
            let left = Rectangle::new(
                (x - bw_phys, y).into(),
                (bw_phys, h_phys).into(),
            );
            // borde derecho: x+w, y, ancho=bw, alto=h
            let right = Rectangle::new(
                (x + w_phys, y).into(),
                (bw_phys, h_phys).into(),
            );

            for segment in [top, bottom, left, right] {
                let _ = frame.draw_solid(segment, &[damage], border_color);
            }
        }
    }

    // Dibujar en orden REVERSO (back-to-front):
    // all_elements está en front-to-back (lo que está al frente viene primero en el Vec).
    // Para que OpenGL pinte correctamente (lo más reciente encima), dibujamos desde el fondo.
    for element in all_elements.iter().rev() {
        let geo = element.geometry(scale);
        let result = element.draw(&mut frame, element.src(), geo, &[damage], &[], None);
        if let Err(ref e) = result {
            info!("[draw] ERROR al dibujar elemento: {:?}", e);
        }
    }

    let _ = frame.finish().unwrap();
    drop(framebuffer);
    state.backend.submit(None).unwrap();

    // send_frames_surface_tree -> avisa a cada cliente que su frame fue mostrado
    let output = state.output.clone();
    // usamos el snapshot que ya teníamos (evita conflicto de borrow con renderer activo)
    for (surface, _rect) in &window_snap {
        // frame callback al toplevel
        send_frames_surface_tree(
            surface.wl_surface(),
            &output,
            start_time.elapsed(),
            None,
            |_, _| Some(output.clone()),
        );
        // frame callbacks a los popups de esta ventana
        // importante: sin esto, los popups nunca reciben la señal de "tu frame fue mostrado"
        // y el cliente queda esperando indefinidamente -> broken pipe / crash
        for (popup, _) in PopupManager::popups_for_surface(surface.wl_surface()) {
            send_frames_surface_tree(
                popup.wl_surface(),
                &output,
                start_time.elapsed(),
                None,
                |_, _| Some(output.clone()),
            );
        }
    }

    // frame callbacks a las layer surfaces (barras, docks, wallpapers)
    for item in &state.layer_surfaces {
        if item.surface.alive() {
            send_frames_surface_tree(
                item.surface.wl_surface(),
                &output,
                start_time.elapsed(),
                None,
                |_, _| Some(output.clone()),
            );
        }
    }

    state.backend.window().request_redraw();
}
