#![allow(unused)]
use fuser::FUSE_ROOT_ID;
use tokio::runtime::Handle;
use std::sync::atomic::{AtomicU64, Ordering};
use reqwest::{Client, header};
use serde_json::{Value, json};
use serde::{Serialize, Deserialize};
use base64::{engine::general_purpose, prelude::*};
use std::collections::{HashMap, HashSet};
use std::io::Result;


#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Repo {
    pub name: String,
    pub size: u64,
    pub filled: RepoStatus,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum RepoStatus {
    Active(u64),
    Sealed,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct File {
    pub name: String,
    pub api: String,
    pub sha: String,
    pub size: u64,
    pub ino: u64,
    pub data: Vec<u8>,
    pub sync: bool,          // if created and not synced with github database
}

pub struct Github {
    pub username: String,
    pub client: Client,
    pub cache: Option<HashMap<Repo, Vec<File>>>,
    pub name: String,
    pub email: String,
}

static CURRENT_INO: AtomicU64 = AtomicU64::new(FUSE_ROOT_ID + 1);
const REPO_SIZE_LIMIT: u64 = 100 * 1024; // in KB
const FILE_SIZE_LIMIT: u64 = 0; // in KB

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
            cache: None,
            name: name,
            email: email
        }
    }

    pub async fn get_repos(&self) -> Vec<Repo> {
        let api = "https://api.github.com/user/repos?visibility=all&sort=created&direction=asc";

        let body = self.client
            .get(api)
            .send()
            .await.unwrap()
            .text()
            .await.unwrap();

        let json: Vec<serde_json::Value> = serde_json::from_str(&body).unwrap();
        let mut found_repos: Vec<Repo> = Vec::new();

        for repo in json {
            let name = repo.get("name").unwrap().to_string().trim_matches('"').to_string();
            let size = repo["size"].as_u64().unwrap_or(0);
            found_repos.push(Repo {
                name: name,
                size: size,
                filled: if size >= REPO_SIZE_LIMIT { RepoStatus::Sealed } else { RepoStatus::Active(REPO_SIZE_LIMIT - size) } // size left
            });
        }

        found_repos
    }

    pub async fn files_in_repo(&self, repo: &str, branch: Option<&str>) -> Vec<File> {
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
        let mut files: Vec<File> = Vec::new();

        for file in tree {
            if file.get("type").and_then(|v| v.as_str()) == Some("blob") {
                let name = file.get("path").and_then(|x| x.as_str()).unwrap().to_string();
                let url = format!(
                    "https://raw.githubusercontent.com/{}/{}/main/{}",
                        self.username, repo, name
                    );
                let sha = file.get("sha").and_then(|x| x.as_str()).unwrap().to_string();
                let size = file.get("size").and_then(|x| Some(x.as_u64().expect("Error parsing"))).expect("Error parsing!");
                files.push(File {
                    name: name,
                    api: url,
                    sha: sha,
                    size: size,
                    ino: CURRENT_INO.load(Ordering::SeqCst),
                    data: Vec::new(),
                    sync: true,
                });
                CURRENT_INO.fetch_add(1, Ordering::SeqCst);
            }
        }

        files
    }

    pub async fn cache_files(&mut self) -> Result<()> {
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

    pub async fn create_repo(&mut self, repo: &str) -> Option<Repo> {
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
            eprintln!("API Error: {}: {:?}", resp.status(), resp.text().await.unwrap()); return None;
        }

        let resp_json = serde_json::from_str::<serde_json::Value>(&resp.text().await.unwrap()).unwrap();
        let repo_size = resp_json
                .get("size")
                .and_then(|v| v.as_u64())
                .expect("'size' key missing!");

        let repo = Repo {
            name: repo.to_string(),
            size: repo_size,
            filled: if repo_size >= REPO_SIZE_LIMIT { RepoStatus::Sealed } else { RepoStatus::Active(REPO_SIZE_LIMIT - repo_size) },
        };

        self.cache.as_mut().unwrap().insert(
            repo.clone(),
            Vec::new()
        );

        Some(repo)
    }

    pub async fn delete_file(&mut self, file: &str) -> bool {
        false
    }

    pub async fn upload_file(&mut self, old_file: Option<File>, content: &str) -> bool {
        // if old_file is none then is just wants to create a file
        false
    }

    pub async fn download_file(&self, file: &str) -> String {
        "".to_string()
    }

}

pub enum FileType {
    File(File),
    Dir(HashMap<String, u64>)
}

pub struct Node {
    pub ino: u64,
    pub name: String,
    pub kind: FileType,
    pub parent: u64,
}

pub struct FileTree {
    pub nodes: HashMap<u64, Node>,
    pub root: u64,
    pub next_ino: u64,
    pub github: Option<Github>,
    pub handle: Handle
}

impl FileTree {
    pub fn new(files: Vec<File>, handle: Handle) -> Self {
        let mut fs = FileTree {
            nodes: HashMap::new(),
            root: FUSE_ROOT_ID,
            next_ino: FUSE_ROOT_ID + 1,
            github: None,
            handle: handle
        };

        fs.nodes.insert(FUSE_ROOT_ID, Node {
            ino: FUSE_ROOT_ID,
            name: "/".into(),
            kind: FileType::Dir(HashMap::new()),
            parent: FUSE_ROOT_ID
        });

        for file in files {
            fs.insert_path(file);
        }

        fs
    }

    fn alloc_ino(&mut self) -> u64 {
        let ino = self.next_ino;
        self.next_ino += 1;
        ino
    }

    pub fn insert_path(&mut self, file: File) {
        let mut current = self.root;
        let path = &file.name;

        let parts: Vec<&str> = path.split("/").collect();

        for (i, part) in parts.iter().enumerate() {
            let is_last = i == parts.len() - 1;

            let next = match &self.nodes[&current].kind {
                FileType::Dir(children) => children.get(*part).copied(),
                _ => unreachable!()
            };

            current = match next {
                Some(ino) => ino,
                None => {
                    let ino = self.alloc_ino();

                    let node = Node {
                        ino,
                        name: part.to_string(),
                        parent: current,
                        kind: if is_last {
                            FileType::File(file.clone())
                        } else {
                            FileType::Dir(HashMap::new())
                        },
                    };

                    if let FileType::Dir(children) =
                        &mut self.nodes.get_mut(&current).unwrap().kind
                    {
                        children.insert(part.to_string(), ino);
                    }

                    self.nodes.insert(ino, node);
                    ino
                }
            };
        }
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
