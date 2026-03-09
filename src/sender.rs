use base64::Engine;
use base64::engine::general_purpose;
use reqwest::{Client, header};
use serde::{Serialize, Deserialize};
use serde_json::json;
use std::fs::{File, OpenOptions};
use std::io::{Read, Write};
use std::path::Path;
use std::collections::HashMap;

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Repo {
    pub id: u64,
    pub name: String,
    pub size: u64,
    pub filled: RepoStatus,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum RepoStatus {
    Active(u64),
    Sealed,
}

pub struct FileStruct {
    pub name: String,
    pub api: String,
    pub size: u64,
    pub sha: String,
    pub chunk_id: u64
}

pub struct Github {
    pub username: String,
    pub client: Client,
    pub name: String,
    pub email: String,
    pub repos: Vec<Repo>,
}

const REPO_SIZE_LIMIT: u64 = 100 * 1024; // in KB
const FILE_SIZE_LIMIT: u64 = 75 * 1024 * 1024; // in mb

impl Github {
    pub fn new(token: &str, username: &str) -> Github {
        let name: String = String::from("cloudserver-id"); // add name
        let email: String = String::from("suckitlilbros@outlook.com"); // add email (same as that of account)

        let mut headers = header::HeaderMap::new();

        headers.insert(
            header::AUTHORIZATION,
            format!("Bearer {}", token).parse().unwrap()
        );

        headers.insert(
            header::USER_AGENT,
            "Mozilla/5.0 (Linux; Android 16; Pixel 9) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/143.0.12.45 Mobile Safari/537.36"
            .parse()
            .unwrap()
        );

        headers.insert(
            header::ACCEPT,
            "application/vnd.github+json".parse().unwrap()
        );

        let client = Client::builder()
            .default_headers(headers)
            .build()
            .unwrap();

        Github {
            username: username.to_string(),
            client: client,
            name: name,
            email: email,
            repos: Vec::new()
        }
    }

    pub async fn get_repos(&mut self) {
        let api = "https://api.github.com/user/repos?visibility=all&sort=created&direction=asc";

        let body = self.client
            .get(api)
            .send()
            .await.unwrap()
            .text()
            .await.unwrap();

        let json: Vec<serde_json::Value> = serde_json::from_str(&body).unwrap();

        for repo in json {
            let name = repo.get("name").unwrap().to_string().trim_matches('"').to_string();
            let size = repo["size"].as_u64().unwrap_or(0);
            self.repos.push(Repo {
                id: name.rsplit('_').next().expect("Error after splitting").parse().expect("Error: parsing the number"),
                name: name,
                size: size,
                filled: if size >= REPO_SIZE_LIMIT { RepoStatus::Sealed } else { RepoStatus::Active(REPO_SIZE_LIMIT - size) } // size left
            });
        }
    }

    pub async fn files_in_repo(&self, repo: &str, branch: Option<&str>) -> HashMap<String, Vec<FileStruct>> {
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

        let mut chunked_file_map: HashMap<String, Vec<FileStruct>> = HashMap::new();

        for file in tree {
            if file.get("type").and_then(|v| v.as_str()) == Some("blob") {
                let name = file.get("path").and_then(|x| x.as_str()).unwrap().to_string();
                let (stem, file_name, cnk_id) = file_metadata(&name).expect("Error");
                let url = format!(
                    "https://raw.githubusercontent.com/{}/{}/main/{}",
                    self.username, repo, name
                );
                let size = file.get("size").and_then(|x| Some(x.as_u64().expect("Error parsing"))).expect("Error parsing!");
                let sha = file.get("sha").and_then(|x| x.as_str()).unwrap().to_string();

                let current_file = FileStruct {
                    name: format!("{}{}", stem, file_name),
                    api: url,
                    size: size,
                    sha: sha,
                    chunk_id: cnk_id,
                };

                chunked_file_map
                    .entry(current_file.name.clone())
                    .or_insert_with(|| Vec::new())
                    .push(current_file);
            }
        }

        chunked_file_map
    }

    pub async fn create_repo(&self, repo: &str) -> Option<Repo> {
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
        let repo_size = resp_json
            .get("size")
            .and_then(|v| v.as_u64())
            .expect("'size' key missing!");

        let repo = Repo {
            id: repo.rsplit('_').next().expect("Error after splitting").parse().expect("Error: parsing the number"),
            name: repo.to_string(),
            size: repo_size,
            filled: if repo_size >= REPO_SIZE_LIMIT { RepoStatus::Sealed } else { RepoStatus::Active(REPO_SIZE_LIMIT - repo_size) },
        };

        Some(repo)
    }


    pub async fn get_suitable_repo(&mut self, file_size: u64) -> Option<usize> {
        for (idx, repo) in self.repos.iter().enumerate() {
            if let RepoStatus::Active(size_left) = repo.get_size() {
                if size_left > file_size {
                    return Some(idx);
                }
            }
        }

        let repo_id = self.repos.iter().last().map(|r| r.id + 1).unwrap_or(0);
        match self.create_repo(&format!("repo_{}", repo_id)).await {
            Some(repo) => {
                self.repos.push(repo);
                return Some(self.repos.len() - 1);
            },
            None => return None
        }
    }

    /*
     * User sends a file
     * Open it get data
     * chunk it into 75KB chunks base64 encode chunks and upload it
     * */
    pub async fn upload_file(&mut self, file_name: &str) {
        let mut content = Vec::new();

        if let Err(err) = File::open(&file_name)
            .expect("Error: Opening File")
            .read_to_end(&mut content) {
            eprintln!("[!] Error: {err}");
            return;
        }

        let chunks = content
            .chunks(FILE_SIZE_LIMIT as usize);
        let total_chunks_len = chunks.len();

        let mut start_chunk_id = 0;
        let mut body = json!({
            "message" : format!("Uploading file {}", file_name),
            "committer" : {
                "name" : self.name,
                "email" : self.email
            },
        });

        for chunk in chunks {
            let chunk_name = format!("{}/{}_chunk_{}", file_name, file_name, start_chunk_id);
            let chunk_len = chunk.len() as u64;
            let repo_idx = self.get_suitable_repo(chunk_len)
                .await
                .expect("Error: Finding or creating a repo");

            let api = format!(
                "https://api.github.com/repos/{}/{}/contents/{}",
                self.username, self.repos[repo_idx].name, chunk_name
            );

            body["content"] = general_purpose::STANDARD.encode(chunk).into();

            let resp = self.client
                .put(&api)
                .json(&body)
                .send()
                .await.expect("Error sending request");

            let status = resp.status();
            let resp_txt = resp.text().await.expect("Error getting text");

            if !status.is_success() {
                eprintln!("API Error: {}: {}", status, resp_txt);
                return;
            }

            start_chunk_id += 1;
            self.repos[repo_idx].set_size(chunk_len);
            println!("[API] UPLOADED FILE CHUNKED {}/{}", start_chunk_id, total_chunks_len);
        }
    }

    pub async fn delete_file(&mut self, required_file: &str) {
        let message = "Commiting to delete the file";

        // Repos already setup
        let mut filesystem = HashMap::new();

        for repo in &self.repos {
            filesystem.insert(repo, self.files_in_repo(&repo.name, None).await);
        }

        let mut body = json!({
            "message" : message,
            "committer" : {
                "name" : self.name,
                "email" : self.email,
            },
        });

        for (repo, files_in_repo) in filesystem {
            for (file_name, file_chunks) in files_in_repo {
                if !file_name.eq(required_file) {
                    continue;
                }

                let (mut start_idx, end_idx) = (0, file_chunks.len());

                // Create the delete request
                for chunk in file_chunks {
                    let api = format!("https://api.github.com/repos/{}/{}/contents/{}",
                        self.username, repo.name, chunk.name
                    );

                    body["sha"] = chunk.sha.into();

                    let resp = self.client
                        .delete(&api)
                        .json(&body)
                        .send()
                        .await.unwrap();

                    if !resp.status().is_success() {
                        eprintln!("[ERROR] Deleting file chunk");
                        return;
                    }

                    start_idx += 1;
                    println!("[API] Deleting File Chunk {}/{}", start_idx, end_idx);
                }
            }
        }
    }

    pub async fn download_file(&self, required_file: &str, output_file: &str) {
        let mut file = OpenOptions::new()
            .write(true)
            .open(output_file)
            .expect("Error: Opening file");

        let mut filesystem = Vec::new();

        for repo in &self.repos {
            filesystem.push(self.files_in_repo(&repo.name, None).await);
        }


        for files_in_repo in filesystem {
            for (file_name, file_chunks) in files_in_repo {
                if !file_name.eq(required_file) {
                    continue;
                }

                let (mut start_idx, end_idx) = (0, file_chunks.len());

                for chunk in file_chunks {
                    let resp = self.client
                        .get(&chunk.api)
                        .send()
                        .await.unwrap();

                    if !resp.status().is_success() {
                        println!("[ERROR] Downloading Chunk");
                        break;
                    }

                    let data = resp.text().await.unwrap();
                    if let Err(err) = file.write_all(data.as_bytes()) {
                        eprintln!("[ERROR] Writting to file: {err}");
                        break;
                    } else {
                        start_idx += 1;
                        println!("[API] Written files chunks {}/{}", start_idx, end_idx);
                    }
                }
            }
        }
    }
}

impl Repo {
    fn set_size(&mut self, size: u64) {
        if let RepoStatus::Active(size_left) = self.filled {
            if size_left - size <= 0 {
                self.filled = RepoStatus::Sealed;
                return;
            }

            self.filled = RepoStatus::Active(size_left - size);
        }
    }

    fn get_size(&self) -> RepoStatus {
        self.filled.clone()
    }
}

fn file_metadata(file: &str) -> Option<(String, String, u64)> {
    let path = Path::new(&file).parent().unwrap();

    let file_stem = path
        .parent()
        .map(|p| format!("{}/", p.to_string_lossy()))
        .unwrap_or_else(|| "".to_string());

    let file_name = path
        .file_name()
        .unwrap()
        .to_string_lossy()
        .to_string();

    let cnk_id: u64 = file
        .rsplit("_chunk_")
        .next()
        .unwrap()
        .parse()
        .unwrap();

    Some((file_stem, file_name, cnk_id))
}
