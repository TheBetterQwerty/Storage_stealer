#![allow(unused)]
use reqwest::{Client, header};
use serde_json::{Value, json};
use serde::{Serialize, Deserialize};
use base64::{engine::general_purpose, prelude::*};
use std::collections::HashMap;
use std::io::Result;

#[derive(Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Repos {
    pub name: String,
    pub size: u64,
}

#[derive(Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Files {
    pub file: String,
    pub api: String,
    pub sha: String,
}

pub struct Github {
    pub username: String,
    pub client: Client,
    pub cache: Option<HashMap<Repos, Vec<Files>>>
    pub name: String,
    pub email: String
}

const CACHE_PATH: &str = ""; // set it to cache path

/*
 * TODO: Remove all unwrap and use expect
 * */

impl Github {
    pub fn new(token: &'static str, user_agent: &str, username: &str) -> Github {
        let name: String = String::new(); // add name
        let email: String = String::new(); // add email (same as that of account)

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

        Github { username: username.to_string(), client: client , cache: None, name, email }
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
                    let sha = file.get("sha").unwrap().to_string();
                    files.push(Files { file: name, api: url , sha });
                }
            }
        }

        files
    }

    pub async fn cache_files(&mut self) -> Result<()> {
        // call on mount
        let repos = self.get_repos().await;
        let mut cache = HashMap::new();

        for repo in repos {
            let files = self.files_in_repo(&repo.name, None).await;
            cache.insert(repo, files);
        }

        self.cache = Some(cache);
        Ok(())
    }

    pub async fn create_repo(&mut self, repo: &str) -> Option<Repos> {
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

        let status_code = resp.status().is_success();

        let resp_json = serde_json::from_str::<serde_json::Value>(&resp.text().await.unwrap()).unwrap();

        let repo = Repos {
            name: repo.to_string(),
            size: resp_json
                .get("size")
                .and_then(|v| v.as_u64())
                .expect("'size' key missing!")
        };

        if status_code {
            self.cache.as_mut().unwrap().insert(
                repo.clone(),
                Vec::new()
            );
        }

        Some(repo)
    }

    pub async fn update_file_content(&mut self, old_file: &Files, repo: &Repos, content: &str) -> bool {
        let api = format!("https://api.github.com/repos/{}/{}/contents/{}", self.username, repo.name, old_file.file);

        let body = json!({
            "message" : format!("Updating file {}", old_file.file),
            "committer" : {
                "name" : self.name,
                "email" : self.email
            },
            "content" : content,
            "sha" : old_file.sha
        });

        let resp = self.client
            .put(api)
            .json(&body)
            .send()
            .await.expect("Error sending request");

        if !resp.status().is_success() {
            return false;
        }

        let resp_json = serde_json::from_str::<serde_json::Value>(&resp
            .text()
            .await
            .expect("[!] Error: getting text from response")
        ).expect("[!] Error: Deserializing");

        let sha_file = resp_json
            .get("sha")
            .expect("[!] Error: No key named 'sha'");

        if let Some(cache) = self.cache.as_mut() {
            if let Some(files) = cache.get_mut(repo) {
                for file in files.iter_mut() {
                    if !file.file.eq(&old_file.file) {
                        continue;
                    }
                    file.sha.clear();
                    file.sha.push_str(sha_file.as_str().expect("Error convering to str"));
                    break;
                }
            }
        }

        true
    }

    pub async fn rename_file(&self) {} // later

    async fn upload_file_content(&mut self, api: &str, file_name: &str, content: &str, repo: &Repos) -> bool {
        let body = json!({
            "message" : format!("Uploading {}", file_name),
            "committer" : {
                "name" : self.name,
                "email" : self.email
            },
            "content" : content,
            "branch" : "main" // to avoid problems
        });

        let resp = self.client
            .put(api)
            .json(&body)
            .send()
            .await.unwrap();

        if !resp.status().is_success() {
            return false;
        }

        let resp_json = serde_json::from_str::<serde_json::Value>(&resp
            .text()
            .await
            .expect("[!] Error: getting text from response")
        ).expect("[!] Error: Deserializing");

        let sha_file = resp_json
            .get("sha")
            .expect("[!] Error: No key named 'sha'");

        // update the file data
        if let Some(cache) = self.cache.as_mut() {
            if let Some(files_in_r) = cache.get_mut(repo) {
                for file in files_in_r.iter_mut() {
                    if !file.file.eq(file_name) {
                        continue;
                    }
                    file.sha.push_str(sha_file.as_str().unwrap());
                    break;
                }
            }
        }

        true
    }

    pub async fn upload_file(&mut self, file: &str) -> bool {
        let data: Vec<u8> = file_contents(file);
        let repositories = self.get_repos().await;

        let repo_name = repositories
            .iter()
            .filter(|r| r.size + (data.len() as u64 / 1024) < 1_048_576)
            .min_by_key(|r| r.size)
            .map(|r| r.clone());

        let (api, final_repo) = match repo_name {
            Some(repo) => {
                let api = format!(
                    "https://api.github.com/repos/{}/{}/contents/",
                    self.username, repo.name
                );

                (api, repo)
            },
            None => {
                let repo_name_new = if let Some(last_repo) = repositories.last() {
                    let number: i32 = last_repo.name
                        .split("_")
                        .nth(1)
                        .expect("repo doesn't contain '_'")
                        .parse()
                        .expect("Failed to parse repo number!");

                    format!("repo_{}", number + 1)
                } else {
                    "repo_1".to_string()
                };

                let new_repo = self.create_repo(&repo_name_new).await;

                if let None = new_repo {
                    eprintln!("[!] Error creating {}", repo_name_new);
                    return false;
                }

                let new_repo = new_repo.unwrap();

                let api = format!(
                    "https://api.github.com/repos/{}/{}/contents/",
                    self.username, new_repo.name
                );

                (api, new_repo)
            }
        };

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
                    &general_purpose::STANDARD.encode(chunk),
                    &final_repo
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
                &general_purpose::STANDARD.encode(data),
                &final_repo
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

fn encrypt_content(data: Vec<u8>, key: Vec<u8>) -> Vec<u8> {
    vec![]
}

fn decrypt_cotent(data: Vec<u8>, key: Vec<u8>, nonce: Vec<u8>) -> Vec<u8> {
    vec![]
}

fn file_contents(file: &str) -> Vec<u8>{
    vec![]
}
