mod github;

const TOKEN: &str = "";
const USER_AGENT: &str = "";
const USER_NAME: &str = "";

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + 'static>> {
    let github = github::Github::new(TOKEN, USER_AGENT, USER_NAME);


    Ok(())
}
