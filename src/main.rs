// main.rs
// punto de entrada
// inicialización -> state.rs (Vinland::new)
// renderizado    -> render.rs
// protocolos     -> handlers/

use std::time::Instant;
use tracing::info;
use calloop::EventLoop;
use smithay::reexports::wayland_server::Display;
use smithay::wayland::compositor::CompositorClientState;

mod state;
mod handlers;
mod render;

use state::{Vinland, ClientState};

fn main() {
    tracing_subscriber::fmt::init();
    info!("iniciando vinland...");

    let start_time = Instant::now();
    let mut event_loop: EventLoop<Vinland> = EventLoop::try_new().unwrap();
    let display: Display<Vinland> = Display::new().unwrap();

    // vinland::new -> inicializa todos los protocolos wayland y el backend
    // &display: display queda en main para pasarlo al generic source de calloop
    let (mut state, winit_evt_loop) = Vinland::new(&display, event_loop.get_signal());
    info!("compositor inicializado");

    let loop_handle = event_loop.handle();

    // fuente 1 -> display wayland (mensajes entrantes de clientes)
    // generic wrappea el fd del display para que calloop lo monitoree
    loop_handle.insert_source(
        smithay::reexports::calloop::generic::Generic::new(
            display,
            calloop::Interest::READ,
            calloop::Mode::Level,
        ),
        |_, display, state| {
            unsafe { display.get_mut().dispatch_clients(state).unwrap() };
            Ok(calloop::PostAction::Continue)
        },
    ).unwrap();

    // fuente 2 -> socket wayland (conexiones nuevas de clientes)
    let socket = smithay::wayland::socket::ListeningSocketSource::new_auto().unwrap();
    info!("socket: {:?}", socket.socket_name());
    loop_handle.insert_source(socket, |stream, _, state| {
        state.display_handle
            .insert_client(
                stream,
                std::sync::Arc::new(ClientState {
                    compositor_state: CompositorClientState::default(),
                }),
            ).unwrap();
    }).unwrap();

    // fuente 3 -> winit
    loop_handle.insert_source(winit_evt_loop, move |event, _, state| {
        match event {
            smithay::backend::winit::WinitEvent::CloseRequested => {
                info!("ventana cerrada");
                state.loop_signal.stop();
            }
            smithay::backend::winit::WinitEvent::Redraw => {
                render::render_frame(state, start_time);
            }
            _ => {}
        }
    }).unwrap();

    // idle callback -> flushea respuestas pendientes a todos los clientes
    event_loop.run(None, &mut state, |state| {
        state.display_handle.flush_clients().unwrap();
    }).unwrap();
}