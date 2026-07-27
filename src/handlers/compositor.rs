//   - compositorhandler -> wl_surface commits
//   - shmhandler -> buffers de memoria compartida (wl_shm)
//   - bufferhandler -> buffer destruido por el cliente

use smithay::wayland::compositor::{CompositorHandler, CompositorState, CompositorClientState};
use smithay::wayland::shm::ShmHandler;
use smithay::wayland::buffer::BufferHandler;
use smithay::reexports::wayland_server::protocol::wl_surface::WlSurface;
use smithay::backend::renderer::utils::on_commit_buffer_handler;

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

        // notificamos al PopupManager: puede haber un popup esperando ser configurado
        // PopupManager.commit() envía el configure inicial a cualquier popup
        // cuya xdg_surface aún no fue configurada (el primer commit lo dispara)
        self.popups.commit(surface);

        // Si la superficie pertenece a una de nuestras ventanas normales (sin padre)
        // y aún no ha sido configurada/tilada (su rect de tamaño es 0):
        let mut should_retile = false;
        for w in self.windows.iter_mut() {
            if w.surface.wl_surface() == surface
                && w.surface.parent().is_none()
                && w.rect.size.w == 0
                && w.rect.size.h == 0
            {
                // Establecemos un tamaño temporal de (1, 1) para marcarla como "lista para tilar"
                // y evitar que vuelva a dispararse en futuros commits
                w.rect.size = (1, 1).into();
                should_retile = true;
                break;
            }
        }

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
