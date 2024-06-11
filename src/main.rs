mod app;
mod state;
mod structs;
mod controllers;

use app::run;

fn main() {
    pollster::block_on(run());
}
