#![allow(unused)]
use reqwest::{Client, header};
use serde_json::{Value, json};
use serde::{Serialize, Deserialize};
use base64::{engine::general_purpose, prelude::*};
use std::collections::HashMap;
use std::io::Result;

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Repos {
    pub name: String,
    pub size: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Files {
    pub file: String,
    pub api: String,
    pub sha: String,
}

pub struct Github {
    pub username: String,
    pub client: Client,
    pub cache: Option<HashMap<Repos, Vec<Files>>>,
    pub name: String,
    pub email: String,
}

#[derive(Deserialize)]
struct BlobResponse {
    content: String,
    encoding: String
}
/*
 * TODO: Remove all unwrap and use expect
 * */

impl Github {
    pub fn new(token: &str, user_agent: &str, username: &str) -> Github {
        let name: String = String::from("cloudserver-id"); // add name
            let email: String = String::from("suckitlilbros@outlook.com"); // add email (same as that of account)

        let mut headers = header::HeaderMap::new();

        headers.insert(
            header::AUTHORIZATION,
            format!("Bearer {}", token).parse().unwrap()
        );

        headers.insert(
            header::USER_AGENT,
            user_agent.parse().unwrap()
        );

        headers.insert(
            header::ACCEPT,
            "application/vnd.github+json".parse().unwrap()
        );

        let client = Client::builder()
            .default_headers(headers)
            .build()
            .unwrap();

        Github { username: username.to_string(), client: client , cache: None, name, email }
    }

    pub async fn get_repos(&self) -> Vec<Repos> {
        let api = "https://api.github.com/user/repos?visibility=all&sort=created&direction=asc";

        let body = self.client
            .get(api)
            .send()
            .await.unwrap()
            .text()
            .await.unwrap();

        let json: Vec<serde_json::Value> = serde_json::from_str(&body).unwrap();
        let mut found_repos: Vec<Repos> = Vec::new();

        for repo in json {
            let name = repo.get("name").unwrap().to_string().trim_matches('"').to_string();
            let size = repo["size"].as_u64().unwrap_or(0);
            found_repos.push(Repos { name, size });
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

        let json: serde_json::Value = serde_json::from_str(&body).unwrap();
        let tree = json["tree"].as_array().unwrap();
        let mut files: Vec<Files> = Vec::new();

        for file in tree {
            if file.get("type").and_then(|v| v.as_str()) == Some("blob") {
                let name = file.get("path").and_then(|x| x.as_str()).unwrap().to_string();
                let url = format!(
                    "https://raw.githubusercontent.com/{}/{}/main/{}",
                        self.username, repo, name
                    );
                let sha = file.get("sha").and_then(|x| x.as_str()).unwrap().to_string();
                files.push(Files { file: name, api: url , sha });
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

        dbg!(&cache);
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


        if !resp.status().is_success() {
            eprintln!("API Error: {}: {:?}", resp.status(), resp.text().await.unwrap());
            return None;
        }

        let resp_json = serde_json::from_str::<serde_json::Value>(&resp.text().await.unwrap()).unwrap();

        let repo = Repos {
            name: repo.to_string(),
            size: resp_json
                .get("size")
                .and_then(|v| v.as_u64())
                .expect("'size' key missing!")
        };

        self.cache.as_mut().unwrap().insert(
            repo.clone(),
            Vec::new()
        );

        Some(repo)
    }

    pub async fn update_file_content(&mut self, old_file: &str, content: &str) -> bool {
        let mut file = None;
        let mut repo = None;
        // handle if the file is split
        if let Some(cache) = self.cache.as_mut() {
            for (_repo, files) in cache.iter_mut() {
                for _file in files.iter_mut() {
                    if !_file.file.eq(old_file) {
                        continue;
                    }
                    repo = Some(_repo);
                    file = Some(_file);
                }
            }
        }

        if let None = file {
            println!("[!] Error: File not found!\n");
            return false;
        }

        let file = file.unwrap(); // safe unwraping
        let repo = repo.unwrap(); // safe unwraping

        let api = format!("https://api.github.com/repos/{}/{}/contents/{}", self.username, repo.name, file.file);

        let body = json!({
            "message" : format!("Updating file {}", file.file),
            "committer" : {
                "name" : self.name,
                "email" : self.email
            },
            "content" : general_purpose::STANDARD.encode(content),
            "sha" : file.sha
        });

        let resp = self.client
            .put(api)
            .json(&body)
            .send()
            .await.expect("Error sending request");

        if !resp.status().is_success() {
            eprintln!("API Error: {}: {}", resp.status(), resp.text().await.unwrap());
            return false;
        }

        let resp_json = serde_json::from_str::<serde_json::Value>(&resp
            .text()
            .await
            .expect("[!] Error: getting text from response")
        ).expect("[!] Error: Deserializing");

        dbg!(&resp_json);

        let sha_file = resp_json["content"]["sha"]
            .to_string()
            .trim_matches('"')
            .to_string();


        file.sha.push_str(&sha_file);

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
            eprintln!("API Error: {}: {}", resp.status(), resp.text().await.unwrap());
            return false;
        }

        let resp_json = serde_json::from_str::<serde_json::Value>(&resp
            .text()
            .await
            .expect("[!] Error: getting text from response")
        ).expect("[!] Error: Deserializing");


        let sha_file = resp_json["content"]["sha"]
            .as_str()
            .expect("[!] Error: No key named 'sha'")
            .to_string();

        let download_file = resp_json["content"]["download_url"]
            .as_str()
            .expect("[!] Error: No key named 'content' 'download_url'")
            .to_string();

        // update the file data
        if let Some(cache) = self.cache.as_mut() {
            if let Some(files_in_r) = cache.get_mut(repo) {
                for file in files_in_r.iter_mut() {
                    if !file.file.eq(file_name) {
                        continue;
                    }
                    file.sha.push_str(&sha_file);
                    return true;
                }

                let file = Files {
                    file: file_name.to_string(),
                    api: download_file,
                    sha: sha_file
                };

                files_in_r.push(file);
            }
        }

        true
    }

    pub async fn upload_file(&mut self, file: &str) -> bool {
        let data: Vec<u8> = read_file(file);
        let repositories = self.get_repos().await;

        let repo_name = repositories
            .iter()
            .filter(|r| r.size + (data.len() as u64 / 1024) < 1_048_576)
            .min_by_key(|r| r.size)
            .map(|r| r.clone());

        let (api, final_repo) = match repo_name {
            Some(repo) => {
                let api = format!(
                    "https://api.github.com/repos/{}/{}/contents",
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
                    "https://api.github.com/repos/{}/{}/contents",
                    self.username, new_repo.name
                );

                (api, new_repo)
            }
        };

        let file = file
            .split("/")
            .last()
            .expect("Error not found other part");

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

    fn find_file(&self, file_name: &str) -> Vec<Files> {
        // loops inside the caches values vectors and finds the file
        let mut found_files = Vec::new();

        if let Some(data) = &self.cache {
            dbg!(&data);
            for (_, files) in data {
                for file in files {
                    if file.file.starts_with(file_name) {
                        found_files.push(file.clone());
                    }
                }
            }
        }

        found_files
    }

    pub async fn download_file(&self, file_name: &str) -> Vec<u8> {
        let files_to_download = self.find_file(file_name);
        dbg!(&files_to_download);
        let mut dec_data = Vec::new();

        for file in files_to_download {
            let body = self.client
                .get(file.api)
                .send()
                .await.unwrap()
                .text()
                .await.unwrap();

            dec_data.extend_from_slice(body.as_bytes());
        }

        dec_data
    }
}

fn encrypt_content(data: Vec<u8>, key: Vec<u8>) -> Vec<u8> {
    /*
     * implement later
     * generate nonce
     * return nonce + data
    */
    data
}

fn decrypt_content(data: &str) -> Vec<u8> {
    /*
     * Data = nonce + data
     * if in chunks
     * get all the chunks
     * get the nonce from the first file
     * */
    /*
     * get key during mount
     * implement later
     * base64 decode
     * data[..12] -> nonce
     * data[12..] -> data
     *
     * */
    vec![]
}

fn read_file(file: &str) -> Vec<u8>{
    let data = match std::fs::read(file) {
        Ok(d) => d,
        Err(err) => panic!("Error: Reading {} {}", file, err)
    };

    encrypt_content(data, vec![])
}
