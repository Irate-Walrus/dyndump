use std::{
    collections::HashMap,
    fs::{self, File},
    path::PathBuf,
    sync::Arc,
};

use anyhow::{anyhow, Result};
use clap::Parser;
use clap_verbosity::{InfoLevel, Verbosity};
use log;
use reqwest::header::{HeaderMap, HeaderName};
use serde::{de::DeserializeOwned, Serialize};
use tokio::{self, task::JoinSet};

pub mod dynamics;

use dynamics::{EntityDefinition, EntitySet, InnerAcessInfo, OuterAcessInfo};

const API_ENDPOINT: &'static str = "/api/data/";

#[derive(Parser, Clone, Debug)]
#[command(version, about, long_about = None)]
pub struct Args {
    /// Dynamics instance e.g. "https://example.crm6.dynamics.com"
    target: String,

    /// HTTP headers e.g. "Cookie: CrmOwinAuth ...;"
    #[arg(short = 'H', long)]
    headers: Vec<String>,

    /// HTTP/SOCKS proxy e.g. "http://localhost:8080"
    #[arg(short, long)]
    proxy: Option<String>,

    /// API version
    #[arg(short, long, default_value = "v9.2")]
    api: String,

    /// Include specified entitysets only
    #[arg(short, long)]
    include: Vec<String>,

    /// Exclude specified entitysets
    #[arg(short, long, default_values_t = ["webresources".to_string(), "audits".to_string()])]
    exclude: Vec<String>,

    /// Disable TLS checks
    #[arg(short = 'k', long)]
    insecure: bool,

    /// Output directory
    #[arg(short, long, default_value = "dump")]
    output_dir: String,

    #[command(flatten)]
    verbose: Verbosity<InfoLevel>,

    /// Page size preference
    #[arg(long, default_value_t = 1000)]
    page_size: u32,

    /// Threads, one thread per entity set
    #[arg(long, default_value_t = 4)]
    threads: u32,
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Arc::new(Args::parse());
    env_logger::builder()
        .format_target(false)
        .format_timestamp(None)
        .filter_level(args.verbose.log_level_filter())
        .target(env_logger::Target::Stdout)
        .init();

    log::trace!("{:?}", &args);

    let client = match build_client(&args) {
        Ok(c) => Arc::new(c),
        Err(e) => {
            log::error!("failed to build HTTP client");
            log::error!("{}", e);
            return Ok(());
        }
    };

    let whoami = request_whoami(&client, &args).await?;

    let systemuser =
        request_entity::<dynamics::SystemUser>(&client, &args, "systemusers", &whoami.user_id)
            .await?;
    log::info!(
        "systemuser [windowsliveid={}, systemuserid={}, title={:?}]",
        &systemuser.windows_live_id,
        &systemuser.system_user_id,
        &systemuser.title
    );

    if args.include.is_empty() {
        let userprivs =
            request_systemuser_privileges(&client, &args, &systemuser.system_user_id).await?;

        for privilege in userprivs.role_privileges.iter() {
            log::info!(
                "roleprivilege [name={}, depth={}, privilegeid={}]",
                &privilege.privilege_name,
                &privilege.depth,
                &privilege.privilege_id
            );
        }
    }

    let definition_set =
        request_entityset::<dynamics::EntityDefinition>(&client, &args, "EntityDefinitions")
            .await?;

    let mut join_set: JoinSet<Result<()>> = JoinSet::new();
    let definitions: Vec<EntityDefinition> = definition_set
        .value
        .clone()
        .into_iter()
        .filter(|d| {
            log::trace!(
                "definition contains {}={}",
                &d.entity_set_name,
                args.include.contains(&d.entity_set_name)
            );
            (args.include.is_empty() || args.include.contains(&d.entity_set_name))
                && !args.exclude.contains(&d.entity_set_name)
        })
        .collect();

    for definition in definitions {
        while join_set.len() >= args.threads as usize {
            //TODO: errors
            let _ = join_set.join_next().await;
        }

        let args = args.clone();
        let client = client.clone();
        let system_user_id = systemuser.system_user_id.clone();

        join_set.spawn(async move {
            dump_entityset(&client, &args, &system_user_id, &definition).await
        });
    }

    join_set.join_all().await;

    Ok(())
}

async fn dump_entityset(
    client: &reqwest::Client,
    args: &Args,
    systemuser_id: &str,
    definition: &EntityDefinition,
) -> Result<()> {
    let result = request_entityset::<HashMap<String, serde_json::Value>>(
        &client,
        &args,
        &definition.entity_set_name,
    )
    .await;

    if let Err(e) = &result {
        log::warn!(
            "entityset failed {} with {}",
            &definition.entity_set_name,
            e
        );
    }

    if let Ok(r) = &result {
        log::info!(
            "dumped entityset {} [count={}]",
            &definition.entity_set_name,
            r.value.len(),
        );

        if let Some(record) = r.value.first() {
            let id_value = match record.get(&definition.primary_id_attribute) {
                Some(r) => r,
                None => return Err(anyhow!("record primary id attribute is null")),
            };

            let record_id = match id_value.as_str() {
                Some(s) => s,
                None => return Err(anyhow!("record primary id attribute is not a string")),
            };

            let access_result = request_record_accessinfo(
                &client,
                &args,
                &definition.logical_name,
                &record_id,
                &systemuser_id,
            )
            .await;

            if let Ok(outer) = access_result {
                let inner: InnerAcessInfo = match serde_json::from_str(&outer.access_info) {
                    Ok(i) => i,
                    Err(e) => {
                        return Err(anyhow!("failed to deserialize inner access info: {}", e))
                    }
                };

                log::info!(
                    "recordprivilege {} [{}]",
                    &definition.entity_set_name,
                    &inner.granted_access_rights
                );
            }
        }
    }
    Ok(())
}

fn build_client(args: &Args) -> Result<reqwest::Client> {
    let mut builder =
        reqwest::Client::builder().default_headers(parse_http_headers(&args.headers)?);

    if args.insecure {
        builder = builder.danger_accept_invalid_certs(true);
    }

    if let Some(proxy) = args.proxy.clone() {
        builder = builder.proxy(reqwest::Proxy::all(proxy.clone())?);
    }

    builder = builder.connection_verbose(true);

    Ok(builder.build()?)
}

async fn request_whoami(client: &reqwest::Client, args: &Args) -> Result<dynamics::WhoAmIResponse> {
    let url = format!("{}{}{}/WhoAmI", args.target, API_ENDPOINT, args.api);
    log::debug!("requesting /WhoAmI from {}", url);

    let response = client.get(&url).send().await?;

    let status = response.status();
    let body = response.text().await?;

    log::debug!("received response {:?} {:?}", &status, &body);

    if !status.is_success() {
        log::error!("api error {}: {}", status, body);
        return Err(anyhow!("request error")); // or custom error
    }

    let whoami = serde_json::from_str::<dynamics::WhoAmIResponse>(&body)?;

    Ok(whoami)
}

async fn request_systemuser_privileges(
    client: &reqwest::Client,
    args: &Args,
    systemuser_id: &str,
) -> Result<dynamics::UserPrivileges> {
    let response = client
        .get(
            args.target.to_owned()
                + API_ENDPOINT
                + &args.api
                + "/systemusers("
                + systemuser_id
                + ")"
                + "/Microsoft.Dynamics.CRM.RetrieveUserPrivileges",
        )
        .send()
        .await?
        .error_for_status()?
        .json::<dynamics::UserPrivileges>()
        .await?;

    Ok(response)
}

async fn request_entity<T: DeserializeOwned>(
    client: &reqwest::Client,
    args: &Args,
    entity_set_name: &str,
    entity_id: &str,
) -> Result<T> {
    let response = client
        .get(
            args.target.to_owned()
                + API_ENDPOINT
                + &args.api
                + "/"
                + entity_set_name
                + "("
                + entity_id
                + ")",
        )
        .send()
        .await?
        .json::<T>()
        .await?;

    Ok(response)
}

async fn request_entityset<T: DeserializeOwned + Serialize>(
    client: &reqwest::Client,
    args: &Args,
    entity_set_name: &str,
) -> Result<EntitySet<T>> {
    let url =
        args.target.to_owned() + API_ENDPOINT + &args.api + "/" + entity_set_name + "?$count=true";

    let mut set = EntitySet::<T> {
        odata_context: "".to_owned(),
        odata_count: -1,
        odata_next: Some(url),
        value: Vec::new(),
    };

    let mut i = 0;
    while let Some(next_url) = set.odata_next {
        log::debug!("dumping page {} of entityset {}", i, &entity_set_name);
        let response = client
            .get(next_url)
            .header("Prefer", format!("odata.maxpagesize={}", args.page_size))
            .send()
            .await?
            .error_for_status()?;

        let mut page = response.json::<EntitySet<T>>().await?;
        log::debug!(
            "dumped page {} of entityset {} [page_size={}]",
            i,
            &entity_set_name,
            &page.value.len()
        );

        set.value.append(&mut page.value);
        set.odata_count = set.value.len() as i64;
        set.odata_next = page.odata_next;
        i += 1;
    }

    fs::create_dir_all(&args.output_dir)?;

    let mut file_path = PathBuf::from(&args.output_dir);
    file_path.push(entity_set_name);
    file_path.set_extension("json");

    let writer = File::create(&file_path)?;
    serde_json::to_writer(writer, &set)?;
    Ok(set)
}

async fn request_record_accessinfo(
    client: &reqwest::Client,
    args: &Args,
    entity_schema_name: &str,
    entity_id: &str,
    systemuser_id: &str,
) -> Result<dynamics::OuterAcessInfo> {
    let response = client
        .get(
            args.target.to_owned()
                + API_ENDPOINT
                + &args.api
                + "/systemusers("
                + systemuser_id
                + ")/Microsoft.Dynamics.CRM.RetrievePrincipalAccessInfo(ObjectId="
                + entity_id
                + ",EntityName='"
                + entity_schema_name
                + "')",
        )
        .send()
        .await?
        .error_for_status()?
        .json::<OuterAcessInfo>()
        .await?;

    Ok(response)
}

fn parse_http_headers(headers: &Vec<String>) -> Result<HeaderMap> {
    let mut header_map = HeaderMap::new();

    for header_str in headers.iter() {
        if let Some((name, value)) = header_str.split_once(':') {
            let name = name.trim();
            let value = value.trim();

            let header_name = HeaderName::from_lowercase(name.to_lowercase().as_bytes())?;
            header_map.insert(header_name, value.parse()?);
        } else {
            return Err(anyhow!(
                "invalid header format (missing colon): {}",
                &header_str[..header_str.len().min(32)]
            ));
        }
    }

    Ok(header_map)
}
