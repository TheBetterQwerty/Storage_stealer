#![allow(unused)]
use fuser::FUSE_ROOT_ID;
use std::io::Write;
use tokio::runtime::Handle;
use std::sync::atomic::{AtomicU64, Ordering};
use std::path::Path;
use reqwest::{Client, header};
use serde_json::json;
use serde::{Serialize, Deserialize};
use base64::{engine::general_purpose, prelude::*};
use std::collections::HashMap;
use std::io::Result;

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

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct File {
    pub name: String,
    pub api: String,
    pub size: u64,
    pub ino: u64,
    pub tmp_file: Option<String>,
    pub cnk_id: u64,
    pub sync: bool,          // if created and not synced with github database
}

pub struct Github {
    pub username: String,
    pub client: Client,
    pub cache: Option<HashMap<Repo, Vec<Vec<File>>>>,
    pub name: String,
    pub email: String,
}

static CURRENT_INO: AtomicU64 = AtomicU64::new(FUSE_ROOT_ID + 1);
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
                id: name.rsplit('_').next().expect("Error after splitting").parse().expect("Error: parsing the number"),
                name: name,
                size: size,
                filled: if size >= REPO_SIZE_LIMIT { RepoStatus::Sealed } else { RepoStatus::Active(REPO_SIZE_LIMIT - size) } // size left
            });
        }

        found_repos
    }

    pub async fn files_in_repo(&self, repo: &str, branch: Option<&str>) -> Vec<Vec<File>> {
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

        let mut chunked_file_map: HashMap<String, Vec<File>> = HashMap::new();

        for file in tree {
            if file.get("type").and_then(|v| v.as_str()) == Some("blob") {
                let name = file.get("path").and_then(|x| x.as_str()).unwrap().to_string();
                let (stem, file_name, cnk_id) = file_metadata(&name).expect("Error");
                let url = format!(
                    "https://raw.githubusercontent.com/{}/{}/main/{}",
                        self.username, repo, name
                    );
                let size = file.get("size").and_then(|x| Some(x.as_u64().expect("Error parsing"))).expect("Error parsing!");

                let current_file = File {
                    name: format!("{}{}", stem, file_name),
                    api: url,
                    size: size,
                    ino: CURRENT_INO.load(Ordering::SeqCst),
                    tmp_file: None,
                    cnk_id: cnk_id,
                    sync: true,
                };

                CURRENT_INO.fetch_add(1, Ordering::SeqCst);

                chunked_file_map
                    .entry(current_file.name.clone())
                    .or_insert_with(|| Vec::new())
                    .push(current_file);
            }
        }

        chunked_file_map
            .into_values()
            .collect()
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
            id: repo.rsplit('_').next().expect("Error after splitting").parse().expect("Error: parsing the number"),
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

    pub async fn delete_file(&mut self, _file: &str) -> bool {
        false
    }

    pub async fn get_suitable_repo(&mut self, size: u64) -> Option<Repo> {
        let repos: Vec<Repo> = self.cache
            .as_ref()
            .expect("Error: No cached saved!")
            .keys()
            .map(|f| f.clone())
            .collect();


        let mut next_id = 0;

        for repo in repos {
            next_id = repo.id;
            // increase repo size after inserting file in upload
            if let RepoStatus::Active(left_size) = repo.filled {
                if left_size > size {
                    return Some(repo.clone());
                }
            }
        }

        return self.create_repo(&format!("repo_{}", next_id + 1)).await;
    }

    pub async fn upload_file(&mut self, file: std::result::Result<&File, String>, content: &[u8], tmp_file_name: Option<String>) -> bool {
        let chunks: Vec<&[u8]> = content
            .chunks(FILE_SIZE_LIMIT as usize)
            .clone()
            .collect();

        let file_name = match file {
            Ok(ref file) => file.name.clone(),
            Err(ref file_nm) => file_nm.clone()
        };

        let mut start_chunk_id: u64 = match file {
            Ok(ref file) => {
                let a = self.cache
                    .as_ref()
                    .expect("No cache")
                    .values()
                    .flat_map(|cnk| cnk.iter())
                    .find(|cache| !cache.is_empty() && cache[0].name == file.name);

                match a.expect("File sent in func but doesnt exists in cache!").last() {
                    Some(x) => x.cnk_id + 1,
                    None => 0
                }
            },
            Err(_) => 0,  // creating a file
        };

        let mut body = json!({
            "message" : format!("Updating file {}", file_name),
            "committer" : {
                "name" : self.name,
                "email" : self.email
            },

        });

        for chunk in chunks {
            let chunk_name = format!("{}/{}_chunk_{}", file_name, file_name, start_chunk_id);
            let req_repo = self.get_suitable_repo(chunk.len() as u64).await.expect("error creating repo!");
            let api = format!(
                "https://api.github.com/repos/{}/{}/contents/{}",
                self.username, req_repo.name, chunk_name);

            body["content"] = general_purpose::STANDARD.encode(chunk).into();

            let resp = self.client
                .put(&api)
                .json(&body)
                .send()
                .await.expect("Error sending request");

            if !resp.status().is_success() {
                eprintln!("API Error: {}: {}", resp.status(), resp.text().await.unwrap());
                return false;
            }

            let resp = resp
                .text()
                .await.expect("Error getting text");

            let _resp_json = serde_json::from_str::<serde_json::Value>(&resp)
                .expect("Error: Deserializing"); // remove

            let file_chunk = File {
                name: chunk_name,
                api: api,
                size: chunk.len() as u64,
                ino: CURRENT_INO.load(Ordering::SeqCst),
                tmp_file: tmp_file_name.clone(),
                cnk_id: start_chunk_id,
                sync: true
            };

            self.cache
                .as_mut()
                .expect("Error: No cache saved")
                .entry(req_repo.clone())
                .or_insert_with(|| Vec::new())
                .push(vec![file_chunk]);

            start_chunk_id += 1;
        }

        true
    }

    pub async fn download_file(&mut self, needle: &str) -> Option<String> {
        let cnk_file_format = format!("{}/{}_chunk_", needle, needle);

        let found_chunks: Vec<&mut Vec<File>> = self.cache
            .as_mut()
            .expect("Error no cache saved!")
            .values_mut()
            .flat_map(|file| file.iter_mut())
            .filter(|file| {
                file.first().map_or(false, |f|
                    f.name == needle || f.name.starts_with(&cnk_file_format)
                )
            })
            .collect();

        if found_chunks.is_empty() {
            return None;
        }

        let tmp_file_name = format!("/tmp/FS/{}", needle);
        let mut tmp_file = std::fs::OpenOptions::new()
            .write(true)
            .append(true)
            .create(true)
            .open(&tmp_file_name)
            .expect("Error tmp file opening");

        for file_chunk in found_chunks {
            for chunk in file_chunk.iter_mut() {
                if chunk.tmp_file.is_some() {
                    continue;
                }

                let body = self.client
                    .get(&chunk.api)
                    .send()
                    .await.unwrap()
                    .text()
                    .await.unwrap()
                    .into_bytes();

                tmp_file.write_all(&body).expect("Error writting to file!");
                chunk.tmp_file = Some(tmp_file_name.clone());
            }
        }

        Some(tmp_file_name)
    }

    pub async fn sync_files(&self) {}
}

#[derive(Clone)]
pub enum FileType {
    File(File),
    Dir(HashMap<String, u64>)
}

#[derive(Clone)]
pub struct Node {
    pub ino: u64,
    pub name: String,
    pub kind: FileType,
    pub parent: u64,
}

#[derive(Clone)]
pub struct FileHandleData {
    pub logical_name: String,
    pub tmp_path: Option<String>,
    pub is_dirty: bool,
    pub size: u64,
}

pub struct FileTree {
    pub nodes: HashMap<u64, Node>,
    pub root: u64,
    pub next_ino: u64,
    pub github: Github,
    pub handle: Handle,
    pub offset: Option<u64>
}


impl FileTree {
    pub fn new(gh: Github, handle: Handle) -> Self {
        let mut fs = FileTree {
            nodes: HashMap::new(),
            root: FUSE_ROOT_ID,
            next_ino: FUSE_ROOT_ID + 1,
            github: gh,
            handle: handle,
            offset: None
        };

        fs.nodes.insert(FUSE_ROOT_ID, Node {
            ino: FUSE_ROOT_ID,
            name: "/".into(),
            kind: FileType::Dir(HashMap::new()),
            parent: FUSE_ROOT_ID
        });

        let logical_files: Vec<File> = {
            let cache = fs.github.cache.as_ref().expect("error no cache");

            cache
                .values()
                .flat_map(|groups| groups.iter())
                .filter(|chunks| !chunks.is_empty())
                .map(|chunk| {
                    let chunk = &chunk[0];

                    let logical_path = chunk
                        .name
                        .rsplit_once('/')
                        .map(|(p,_)| p)
                        .expect("Invalid chunk path format");

                    let mut f = chunk.clone();
                    f.name = logical_path.to_string();
                    f
                })
            .collect()
        };

        for file in logical_files {
            fs.insert_path(file);
        }

        fs
    }

    pub fn alloc_ino(&mut self) -> u64 {
        let ino = self.next_ino;
        self.next_ino += 1;
        ino
    }

    fn insert_path(&mut self, file: File) {
        let mut current = self.root;
        let parts: Vec<&str> = file
            .name
            .split('/')
            .filter(|p| !p.is_empty())
            .collect();

        for (i, part) in parts.iter().enumerate() {
            let is_last = i == parts.len() - 1;

            let next = match &self.nodes[&current].kind {
                FileType::Dir(children) => children.get(*part).copied(),
                _ => unreachable!("Parent is not a directory"),
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

fn encrypt_content(data: Vec<u8>, _key: Vec<u8>) -> Vec<u8> {
    /*
     * implement later
     * generate nonce
     * return nonce + data
    */
    data
}

fn decrypt_content(_data: &str) -> Vec<u8> {
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
