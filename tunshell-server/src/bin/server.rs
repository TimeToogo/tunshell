use tokio;
use tunshell_server::say_hello;

#[tokio::main]
async fn main() -> () {
    say_hello();
}
