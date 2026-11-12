//! SQLite sürücüsü: stdio üzerinde JSON-RPC (bkz. `less-rs/docs/DRIVERS.md`).

mod driver;

fn main() {
    less_db::serve::run_stdio(Box::new(driver::SqliteDriver::default()));
}
