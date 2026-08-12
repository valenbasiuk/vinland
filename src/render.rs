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
use smithay::desktop::utils::send_frames_surface_tree;
use smithay::desktop::PopupManager;
use smithay::input::pointer::{CursorImageStatus, CursorImageSurfaceData};
use smithay::utils::IsAlive;
use smithay::utils::{Physical, Rectangle, Scale, Transform};

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

    for window in &state.windows {
        // si la ventana está minimizada, no la dibujamos
        if window.minimized {
            continue;
        }
        // si la ventana no ha sido tilada aún (rect w/h == 0), no la dibujamos
        if window.rect.size.w == 0 || window.rect.size.h == 0 {
            continue;
        }
        // to_physical_precise_round: convierte i32 logical a i32 physical sin perder precisión
        let pos = window.rect.loc.to_physical_precise_round(scale);
        let elems: Vec<WaylandSurfaceRenderElement<GlesRenderer>> =
            render_elements_from_surface_tree(
                renderer,
                window.surface.wl_surface(),
                pos,
                scale,
                1.0,
                Kind::Unspecified,
            );
        window_elements.extend(elems);

        // 1b. popups xdg de esta ventana (PopupManager)
        for (popup, popup_location) in PopupManager::popups_for_surface(window.surface.wl_surface())
        {
            let popup_geo_loc = popup.geometry().loc;
            let popup_loc = window.rect.loc + popup_location - popup_geo_loc;
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

        // Calcular el rectángulo destino (Physical) según el modo de escala
        let dest: Rectangle<i32, Physical> = match state.config.background.wallpaper_mode {
            ScaleMode::Stretch => Rectangle::new((0, 0).into(), (size.w, size.h).into()),
            ScaleMode::Fill => {
                // escalar al mayor factor posible (sin bandas, recortando)
                let scale_x = out_w / img_w;
                let scale_y = out_h / img_h;
                let s = scale_x.max(scale_y);
                let sw = (img_w * s) as i32;
                let sh = (img_h * s) as i32;
                let ox = (size.w - sw) / 2;
                let oy = (size.h - sh) / 2;
                Rectangle::new((ox, oy).into(), (sw, sh).into())
            }
            ScaleMode::Fit => {
                // escalar al menor factor (bandas del color de fondo a los costados)
                let scale_x = out_w / img_w;
                let scale_y = out_h / img_h;
                let s = scale_x.min(scale_y);
                let sw = (img_w * s) as i32;
                let sh = (img_h * s) as i32;
                let ox = (size.w - sw) / 2;
                let oy = (size.h - sh) / 2;
                Rectangle::new((ox, oy).into(), (sw, sh).into())
            }
            ScaleMode::Center => {
                // sin escalar, solo centrar
                let ox = (size.w - tex_size.w) / 2;
                let oy = (size.h - tex_size.h) / 2;
                Rectangle::new((ox, oy).into(), (tex_size.w, tex_size.h).into())
            }
            ScaleMode::Tile => {
                // un solo tile centrado por ahora
                // (tile real requeriría múltiples draw calls)
                let ox = (size.w - tex_size.w) / 2;
                let oy = (size.h - tex_size.h) / 2;
                Rectangle::new((ox, oy).into(), (tex_size.w, tex_size.h).into())
            }
        };

        // src = todo el buffer de la imagen
        let src = smithay::utils::Rectangle::new(
            (0.0, 0.0).into(),
            (tex_size.w as f64, tex_size.h as f64).into(),
        );

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
            info!("[vinpaper] error al dibujar textura: {:?}", e);
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
    for window in &state.windows {
        // frame callback al toplevel
        send_frames_surface_tree(
            window.surface.wl_surface(),
            &output,
            start_time.elapsed(),
            None,
            |_, _| Some(output.clone()),
        );
        // frame callbacks a los popups de esta ventana
        // IMPORTANTE: sin esto, los popups nunca reciben la señal de "tu frame fue mostrado"
        // y el cliente queda esperando indefinidamente -> broken pipe / crash
        for (popup, _) in PopupManager::popups_for_surface(window.surface.wl_surface()) {
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
