//   - compositorhandler -> wl_surface commits
//   - shmhandler -> buffers de memoria compartida (wl_shm)
//   - bufferhandler -> buffer destruido por el cliente

use smithay::wayland::compositor::{CompositorHandler, CompositorState, CompositorClientState};
use smithay::wayland::shm::ShmHandler;
use smithay::wayland::buffer::BufferHandler;
use smithay::reexports::wayland_server::protocol::wl_surface::WlSurface;
use smithay::backend::renderer::utils::{on_commit_buffer_handler, with_renderer_surface_state};

use crate::state::{Vinland, ClientState};

// compositorhandler -> wl_surface commits
impl CompositorHandler for Vinland {
    fn compositor_state(&mut self) -> &mut CompositorState {
        &mut self.compositor_state
    }

    // devuelve el estado del compositor para un cliente específico
    fn client_compositor_state<'a>(
        &self,
        client: &'a smithay::reexports::wayland_server::Client,
    ) -> &'a CompositorClientState {
        &client.get_data::<ClientState>().unwrap().compositor_state
    }

    fn commit(&mut self, surface: &WlSurface) {
        // registra el buffer del cliente en el estado interno de Smithay
        // sin esto, render_elements_from_surface_tree no puede importar el buffer
        on_commit_buffer_handler::<Self>(surface);

        // Si la superficie pertenece a una de nuestras ventanas normales (sin padre),
        // aún no ha sido tilada (rect de tamaño 0), y el cliente acaba de adjuntar
        // su primer buffer real de dibujo:
        let should_retile = self.windows.iter().any(|w| {
            w.surface.wl_surface() == surface
                && w.surface.parent().is_none()
                && w.rect.size.w == 0
                && w.rect.size.h == 0
                && with_renderer_surface_state(surface, |renderer_state| {
                    renderer_state.buffer().is_some()
                }).unwrap_or(false)
        });

        if should_retile {
            self.retile();
        }

        // pedimos redibujo para que el próximo frame muestre el contenido del cliente
        self.backend.window().request_redraw();
    }
}

// shmhandler -> buffers de memoria compartida (wl_shm)
impl ShmHandler for Vinland {
    fn shm_state(&self) -> &smithay::wayland::shm::ShmState {
        &self.shm_state
    }
}

// bufferhandler -> buffer destruido por el cliente
impl BufferHandler for Vinland {
    fn buffer_destroyed(
        &mut self,
        _buffer: &smithay::reexports::wayland_server::protocol::wl_buffer::WlBuffer,
    ) {
        // todo: liberar texturas importadas de este buffer.
    }
}
