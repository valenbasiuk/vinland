use tracing::info;
use tracing_subscriber;
use calloop::EventLoop;
use smithay::reexports::wayland_server::Display;
// struct del estado (state.rs)
mod state;
use state::Vinland;


fn main() {
    // sistema de logs
    tracing_subscriber::fmt::init();
    info!("iniciando vinland...");


    // declara eventloop y display
    let mut event_loop: EventLoop<Vinland> = EventLoop::try_new()
        .expect("failed to initialize event loop");

        // inicio del struc para definir inicializaciones
    let display: Display<Vinland> = Display::new().unwrap();
    let display_handle = display.handle();
    let compositor_state = smithay::wayland::compositor::CompositorState::new::<Vinland>(&display_handle);
    let mut state = Vinland {display_handle, compositor_state};

    info!("display wayland creado");

    //loop handler, conecta display al loop
    let loop_handle = event_loop.handle();
loop_handle
    .insert_source(
        smithay::reexports::calloop::generic::Generic::new(
            display,
            calloop::Interest::READ,
            calloop::Mode::Level,
        ),


        // event handler, cuando hay un evento, se ejecuta esta función
        |_, _, _state| {
            Ok(calloop::PostAction::Continue)
        },
    )
    .unwrap();

    info!("Display conectado al event loop");

        let socket = smithay::wayland::socket::ListeningSocketSource::new_auto().unwrap();
    let socket_name = socket.socket_name().to_os_string();
    info!("socket creado: {:?}", socket_name);

    loop_handle
        .insert_source(socket, |client_stream, _, state| {
            state
                .display_handle
                .insert_client(client_stream, std::sync::Arc::new(()))
                .unwrap();
        })
        .unwrap();

    event_loop.run(None, &mut state, |_| {}).unwrap();
}