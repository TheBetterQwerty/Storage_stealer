#![allow(unused)]
use reqwest::{Client, header};
use serde_json::{Value, json};
use base64::{engine::general_purpose, prelude::*};

pub struct Repos {
    pub name: String,
    pub size: u64,
}

pub struct Files {
    pub file: String,
    pub api: String
}

pub struct Github {
    pub username: String,
    pub client: Client,
}

impl Github {
    pub fn new(token: &'static str, user_agent: &str, username: &str) -> Github {
        let mut headers = header::HeaderMap::new();

        headers.insert(
            header::AUTHORIZATION,
            format!("Bearer {}", token).parse().unwrap()
        );

        headers.insert(
            header::USER_AGENT,
            user_agent.parse().unwrap()
        );

        let client = Client::builder()
            .default_headers(headers)
            .build()
            .unwrap();

        Github { username: username.to_string(), client }
    }

    pub async fn get_repos(&self) -> Vec<Repos> {
        let api: String = format!("https://api.github.com/users/{}/repos?sort=created&direction=asc", self.username);

        let body = self.client
            .get(api)
            .send()
            .await.unwrap()
            .text()
            .await.unwrap();

        let json = serde_json::from_str::<serde_json::Value>(&body).unwrap();

        let mut found_repos: Vec<Repos> = Vec::new();

        if let Value::Array(repos) = json {
            for repo in repos {
                let name = repo.get("name").unwrap().to_string();
                let size = repo["size"].as_u64().unwrap_or(0);
                found_repos.push(Repos { name, size });
            }
        }

        found_repos
    }

    pub async fn files_in_repo(&self, repo: &str, branch: Option<&str>) -> Vec<Files> {
        let api: String = format!(
            "https://api.github.com/repos/{}/{}/git/trees/{}?recursive=1",
            self.username,
            repo,
            branch.unwrap_or("main")
        );

        let body = self.client
            .get(api)
            .send()
            .await.unwrap()
            .text()
            .await.unwrap();

        let json = serde_json::from_str::<serde_json::Value>(&body).unwrap();
        let mut files: Vec<Files> = Vec::new();

        if let Value::Array(_files) = json {
            for file in _files {
                if file.get("type").unwrap().eq("blob") {
                    let name = file.get("path").unwrap().to_string();
                    let url = file.get("url").unwrap().to_string();
                    files.push(Files { file: name, api: url });
                }
            }
        }

        files
    }

    pub async fn create_repo(&self, repo: &str) -> bool {
        let body = json!({
            "name" : repo,
            "description" : "",
            "private" : true
        });

        let api = "https://api.github.com/user/repos";

        let resp = self.client
            .post(api)
            .json(&body)
            .send()
            .await.unwrap();

        resp.status().is_success()
    }

    async fn upload_file_content(&self, api: &str, file_name: &str, content: &str) -> bool {
        let name: &str = ""; // add name
        let email: &str = ""; // add email (same as that of account)

        let body = json!({
            "message" : format!("Uploading {}", file_name),
            "committer" : {
                "name" : name,
                "email" : email
            },
            "content" : content
        });

        let resp = self.client
            .post(api)
            .json(&body)
            .send()
            .await.unwrap();

        resp.status().is_success()
    }

    pub async fn upload_file(&self, file: &str) -> bool {
        let mut repo: Option<String> = None;
        let data: Vec<u8> = file_contents(file);
        let repositories = self.get_repos().await;
            for repos in repositories.iter() {
            if repos.size + (data.len() as u64 / 1024) < 1_048_576 { // Comparision in kb
                repo = Some(repos.name.clone());
            }
        }

        let mut api = String::new();

        if let Some(repo_name) = repo {
            api = format!(
                "https://api.github.com/repos/{}/{}/contents/",
                self.username, repo_name
            );
        } else {
            if let Some(repo_name) = repositories.last() {
                let number: i32 = repo_name.name
                    .split("_")
                    .nth(1)
                    .expect("[!] repo doesnt contain '_'")
                    .parse()
                    .expect("[!] Failed to parse repo number!");

                let repo_name_new = format!("repo_{}", number + 1);
                if !self.create_repo(&repo_name_new).await {
                    println!("[!] Error creating {}", repo_name_new);
                    return false;
                }

                api = format!(
                    "https://api.github.com/repos/{}/{}/contents/",
                    self.username, repo_name_new
                );
            } else {
                if !self.create_repo("repo_1").await {
                    println!("[!] Error creating repo_1");
                    return false;
                }

                api = format!(
                    "https://api.github.com/repos/{}/{}/contents/",
                    self.username, "repo_1"
                );
            }
        }

        if data.len() > (100 * 1024 * 1024) { // 100mb
            // chunck the data
            let chunked_data: Vec<Vec<u8>> = data
                .chunks(75 * 1024 * 1024) // 75mb -> encoded increases size
                .map(|c| c.to_vec())
                .collect();

            for (idx, chunk) in chunked_data.iter().enumerate() {
                if !self.upload_file_content(
                    &format!("{}/{}/chunk_{}", api, file, idx + 1),
                    &format!("{}.chunk_{}", file, idx + 1),
                    &general_purpose::STANDARD.encode(chunk)
                ).await {
                    eprintln!("[!] Error: Uploading {}", file);
                    return false;
                }
            }
            return true;
        } else {
            if !self.upload_file_content(
                &format!("{}/{}", api, file),
                file,
                &general_purpose::STANDARD.encode(data)
            ).await {
                eprintln!("[!] Error: Uploading {}", file);
                return false;
            }
            return true;
        }
    }

    pub async fn download_file(&self, file: Files) -> Vec<u8> {
        let body = self.client
            .get(file.api)
            .send()
            .await.unwrap()
            .text()
            .await.unwrap();

        let json = serde_json::from_str::<serde_json::Value>(&body).unwrap();

        let content = json["content"].as_str().unwrap();

        let bytes = general_purpose::STANDARD
            .decode(content.replace("\n", ""))
            .unwrap();

        bytes
    }
}

fn file_contents(file: &str) -> Vec<u8>{
    vec![]
}
