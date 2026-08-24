# this is not intended to be used.
its mostly an experiment to learn about Rust, how compositors work and the low level connection between the kernel and the screen

# whats it for right now/what i want to achieve

for now, it works as a basic tiling compositor for native wayland apps. it handles window layout, inputs (mouse/keyboard) and dropdowns/popups (including grabs, focus states and timing fixes)
workspaces are sketchy but they work, supporting up to 999 and program forwarding. app borders might not be accurate

final project would be to make this into a simple DE with a lightning fast compositor with a focus on simplicity to try and
match the commercial limits on low latency compositors.

name comes from the Vinland region from the game "Fear and Hunger"
