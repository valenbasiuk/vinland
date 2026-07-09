// render.rs
// lógica de renderizado de un frame

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
use smithay::input::pointer::{CursorImageStatus, CursorImageSurfaceData};
use smithay::utils::IsAlive;

use crate::state::Vinland;

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
    let scale  = Scale::from(1.0);

    // ocultar el cursor del host (Winit) si estamos dibujando un cursor personalizado
    let cursor_visible = !matches!(state.cursor_status, CursorImageStatus::Surface(_));
    state.backend.window().set_cursor_visible(cursor_visible);

    // bind() -> prepara el renderer y obtiene el framebuffer del frame actual
    let (renderer, mut framebuffer) = state.backend.bind().unwrap();

    // 1. colectar elementos de las ventanas en su posición de tiling
    let mut all_elements: Vec<WaylandSurfaceRenderElement<GlesRenderer>> = Vec::new();
    for window in &state.windows {
        // to_physical_precise_round: convierte i32 logical a i32 physical sin perder precisión
        let pos = window.rect.loc.to_physical_precise_round(scale);
        let elems = render_elements_from_surface_tree(
            renderer,
            window.surface.wl_surface(),
            pos,
            scale,
            1.0,
            Kind::Unspecified,
        );
        all_elements.extend(elems);
    }

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
        let cursor_elems = render_elements_from_surface_tree(
            renderer,
            surface,
            cursor_pos,
            scale,
            1.0,
            Kind::Cursor,
        );
        all_elements.extend(cursor_elems);
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

    for element in &all_elements {
        let _ = element.draw(
            &mut frame,
            element.src(),
            element.geometry(scale),
            &[damage],
            &[],
            None,
        );
    }

    let _ = frame.finish().unwrap();
    drop(framebuffer);
    state.backend.submit(None).unwrap();

    // send_frames_surface_tree -> avisa a cada cliente que su frame fue mostrado
    let output = state.output.clone();
    for window in &state.windows {
        send_frames_surface_tree(
            window.surface.wl_surface(),
            &output,
            start_time.elapsed(),
            None,
            |_, _| Some(output.clone()),
        );
    }

    state.backend.window().request_redraw();
}
