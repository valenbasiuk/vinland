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
    let mut event_loop: EventLoop<()> = EventLoop::try_new()
        .expect("failed to initialize event loop");

    let mut display: Display<()> = Display::new().unwrap();

    let _display_handle = display.handle();
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
    event_loop.run(None, &mut (), |_| {}).unwrap();
}