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

use smithay::backend::renderer::Renderer;
use smithay::backend::renderer::Frame;
use smithay::backend::renderer::gles::GlesRenderer;
use smithay::backend::renderer::element::{Element, RenderElement, Kind};
use smithay::backend::renderer::element::surface::{
    render_elements_from_surface_tree, WaylandSurfaceRenderElement,
};
use smithay::utils::{Scale, Transform};
use smithay::desktop::utils::send_frames_surface_tree;
use smithay::desktop::PopupManager;
use smithay::input::pointer::{CursorImageStatus, CursorImageSurfaceData};
use smithay::utils::IsAlive;

use crate::state::Vinland;
use tracing::info;

// render_frame -> dibuja un frame completo y avisa a los clientes
//   1. colecta elementos de cada superficie wayland
//   2. limpia el fondo
//   3. dibuja cada elemento
//   4. envía el frame a la pantalla
//   5. manda frame callbacks a los clientes
// TODO: cuando haga composicion realmente (tiling) hay que pensar en los damage rects
pub fn render_frame(state: &mut Vinland, start_time: Instant) {
    let size   = state.backend.window_size();
    let damage = smithay::utils::Rectangle::new((0, 0).into(), size);
    let scale  = Scale::from(state.backend.scale_factor());

    // ocultar el cursor del host (Winit) si estamos dibujando un cursor personalizado
    let cursor_visible = !matches!(state.cursor_status, CursorImageStatus::Surface(_));
    state.backend.window().set_cursor_visible(cursor_visible);

    // bind() -> prepara el renderer y obtiene el framebuffer del frame actual
    let (renderer, mut framebuffer) = state.backend.bind().unwrap();

    // 1. colectar elementos de las ventanas en su posición de tiling
    // all_elements está en orden FRONT-TO-BACK: lo que está más al frente va primero.
    // Al dibujar en reverso, lo más atrás se pinta primero y lo más adelante encima.
    //
    // Capas (de frente a atrás):
    //   [cursor] → [popups xdg] → [ventanas + sus subsurfaces]
    //
    // render_elements_from_surface_tree devuelve la ventana con sus subsurfaces ya
    // ordenadas en front-to-back internamente (above_sub, toplevel, below_sub).

    let mut window_elements: Vec<WaylandSurfaceRenderElement<GlesRenderer>> = Vec::new();
    let mut popup_elements:  Vec<WaylandSurfaceRenderElement<GlesRenderer>> = Vec::new();

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
        let elems: Vec<WaylandSurfaceRenderElement<GlesRenderer>> = render_elements_from_surface_tree(
            renderer,
            window.surface.wl_surface(),
            pos,
            scale,
            1.0,
            Kind::Unspecified,
        );
        window_elements.extend(elems);

        // 1b. popups xdg de esta ventana (PopupManager)
        // popup_location de popups_for_surface ya está en coordenadas de GEOMETRÍA del parent.
        // La posición del surface del popup es: geometry_global + popup_location - popup_geo_loc
        // (popup_geo_loc ajusta el offset interno del popup, como la sombra/padding propio del popup)
        for (popup, popup_location) in PopupManager::popups_for_surface(window.surface.wl_surface()) {
            let popup_geo_loc = popup.geometry().loc;
            // popup_location está en coordenadas de geometría del parent, y window.rect.loc es
            // la posición global de esa geometría → no sumamos window_geo_loc aquí.
            let popup_loc = window.rect.loc + popup_location - popup_geo_loc;
            let popup_pos = popup_loc.to_physical_precise_round(scale);
            let popup_elems: Vec<WaylandSurfaceRenderElement<GlesRenderer>> = render_elements_from_surface_tree(
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

    // Construir all_elements en front-to-back: popups adelante, ventanas atrás.
    // El cursor se prepend más adelante (frente de todo).
    let mut all_elements: Vec<WaylandSurfaceRenderElement<GlesRenderer>> = Vec::new();
    all_elements.extend(popup_elements);
    all_elements.extend(window_elements);

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
            states.data_map
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
        let mut cursor_elems: Vec<WaylandSurfaceRenderElement<GlesRenderer>> = render_elements_from_surface_tree(
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

    // fondo violeta oscuro/magenta %
    frame.clear(
        smithay::backend::renderer::Color32F::from([0.0, 0.0, 0.0, 1.0]),
        &[damage],
    ).unwrap();

    // Dibujar en orden REVERSO (back-to-front):
    // all_elements está en front-to-back (lo que está al frente viene primero en el Vec).
    // Para que OpenGL pinte correctamente (lo más reciente encima), dibujamos desde el fondo.
    for element in all_elements.iter().rev() {
        let geo = element.geometry(scale);
        let result = element.draw(
            &mut frame,
            element.src(),
            geo,
            &[damage],
            &[],
            None,
        );
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

    state.backend.window().request_redraw();
}
