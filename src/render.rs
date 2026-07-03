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
use smithay::utils::{Scale, Point, Physical, Transform};
use smithay::desktop::utils::send_frames_surface_tree;

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

    // bind() -> prepara el renderer y obtiene el framebuffer del frame actual
    let (renderer, mut framebuffer) = state.backend.bind().unwrap();

    // render_elements_from_surface_tree -> importa el buffer del cliente al renderer
    let mut all_elements: Vec<WaylandSurfaceRenderElement<GlesRenderer>> = Vec::new();
    for window in &state.windows {
        let elems = render_elements_from_surface_tree(
            renderer,
            window.wl_surface(),
            Point::<i32, Physical>::from((0, 0)),
            scale,
            1.0,
            Kind::Unspecified,
        );
        all_elements.extend(elems);
    }

    // flipped180 -> compensa que opengl tiene el eje Y invertido en wayland
    let mut frame = renderer
        .render(&mut framebuffer, size, Transform::Flipped180)
        .unwrap();

    // fondo negro
    frame.clear(
        smithay::backend::renderer::Color32F::from([0.5, 0.0, 0.5, 1.0]),
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
    // el cliente espera este callback para saber cuándo mandar el próximo frame
    let output = state.output.clone();
    for window in &state.windows {
        send_frames_surface_tree(
            window.wl_surface(),
            &output,
            start_time.elapsed(),
            None,
            |_, _| Some(output.clone()),
        );
    }

    state.backend.window().request_redraw();
}
