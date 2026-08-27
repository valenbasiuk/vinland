// handlers/layer_shell.rs
// implementación de zwlr_layer_shell_v1
//
// El protocolo layer shell permite a apps como barras, docks y fondos crear
// superficies que el compositor posiciona en "capas" definidas:
//   Background (fondo) < Bottom < ventanas < Top < Overlay
//
// Cada LayerSurface declara:
//   - en qué layer quiere vivir (Layer::Background, Bottom, Top, Overlay)
//   - si se ancla a un borde (Anchor::Top | Right | Bottom | Left)
//   - si ocupa una "exclusive zone" (le pide al compositor que no coloque
//     ventanas en ese espacio, ej: una barra de 30px en el top)
//   - si quiere keyboard focus o no

use smithay::{
    reexports::wayland_server::protocol::wl_output::WlOutput,
    utils::{Logical, Rectangle, Size},
    wayland::{
        compositor::with_states,
        shell::wlr_layer::{
            Anchor, Layer, LayerSurface, LayerSurfaceCachedState,
            WlrLayerShellHandler, WlrLayerShellState,
        },
    },
};
use tracing::info;

use crate::state::{LayerSurfaceItem, Vinland};

impl WlrLayerShellHandler for Vinland {
    fn shell_state(&mut self) -> &mut WlrLayerShellState {
        &mut self.layer_shell_state
    }

    fn new_layer_surface(
        &mut self,
        surface: LayerSurface,
        _output: Option<WlOutput>,
        layer: Layer,
        namespace: String,
    ) {
        info!(
            "[LAYER] nueva layer surface: namespace={:?} layer={:?}",
            namespace, layer
        );

        // Calculamos el tamaño lógico del output para pasárselo a la app.
        // La app responderá con su preferred size/anchor/exclusive_zone.
        let output_size = self
            .output
            .current_mode()
            .map(|m| m.size.to_logical(1))
            .unwrap_or_else(|| Size::from((1920, 1080)));

        surface.with_pending_state(|state| {
            state.size = Some(output_size);
        });
        surface.send_configure();

        self.layer_surfaces.push(LayerSurfaceItem { surface, layer });
    }

    fn layer_destroyed(&mut self, surface: LayerSurface) {
        info!("[LAYER] layer surface destruida");
        self.layer_surfaces.retain(|item| item.surface != surface);
        self.retile();
        self.backend.window().request_redraw();
    }
}

/// Calcula el rectángulo global (en coordenadas lógicas) de una LayerSurface
/// según su anchor, margins y exclusive_zone, dado el tamaño del output.
///
/// Retorna `None` si la superficie todavía no ha mandado su primer commit.
pub fn layer_surface_geometry(
    surface: &LayerSurface,
    output_size: Size<i32, Logical>,
) -> Option<Rectangle<i32, Logical>> {
    // Leer el estado cached que la app mandó
    let (size, anch, margins) = with_states(surface.wl_surface(), |states| {
        let mut binding = states.cached_state.get::<LayerSurfaceCachedState>();
        let s = binding.current();
        let size = s.size;
        let anch = s.anchor;
        let margins = s.margin;
        (size, anch, margins)
    });

    // Si la app no mandó un tamaño todavía, no la posicionamos
    if size.w == 0 && size.h == 0 {
        return None;
    }

    let out_w = output_size.w;
    let out_h = output_size.h;
    let sw = size.w;
    let sh = size.h;

    // Calcular x según anchor horizontal
    let x = if anch.contains(Anchor::LEFT) && !anch.contains(Anchor::RIGHT) {
        margins.left
    } else if anch.contains(Anchor::RIGHT) && !anch.contains(Anchor::LEFT) {
        out_w - sw - margins.right
    } else {
        // stretch o centrado
        margins.left
    };

    // Si stretch horizontal: ocupar todo el ancho menos márgenes
    let w = if anch.contains(Anchor::LEFT) && anch.contains(Anchor::RIGHT) {
        out_w - margins.left - margins.right
    } else {
        sw
    };

    // Calcular y según anchor vertical
    let y = if anch.contains(Anchor::TOP) && !anch.contains(Anchor::BOTTOM) {
        margins.top
    } else if anch.contains(Anchor::BOTTOM) && !anch.contains(Anchor::TOP) {
        out_h - sh - margins.bottom
    } else {
        margins.top
    };

    // Si stretch vertical: ocupar todo el alto menos márgenes
    let h = if anch.contains(Anchor::TOP) && anch.contains(Anchor::BOTTOM) {
        out_h - margins.top - margins.bottom
    } else {
        sh
    };

    Some(Rectangle::new((x, y).into(), (w, h).into()))
}
