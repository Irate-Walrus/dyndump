use std::{
    collections::HashMap,
    fs::{self, File},
    io::Write,
    path::PathBuf,
};

use anyhow::{anyhow, Result};
use clap::Parser;
use clap_verbosity::{InfoLevel, Verbosity};
use log;
use reqwest::header::{HeaderMap, HeaderName};
use serde::{de::DeserializeOwned, Serialize};
use tokio;

use dyndump::dynamics::{self, EntitySet, InnerAcessInfo, OuterAcessInfo};

const API_ENDPOINT: &'static str = "/api/data/";

#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
pub struct Args {
    /// Dynamics instance e.g. "https://example.crm6.dynamics.com"
    target: String,

    /// HTTP headers e.g. "Cookie: CrmOwinAuth ...;"
    #[arg(short = 'H', long)]
    headers: Vec<String>,

    /// HTTP proxy e.g. "http://localhost:8080"
    #[arg(short, long)]
    proxy: Option<String>,

    /// API version
    #[arg(short, long, default_value = "v9.2")]
    api: String,

    /// Disable TLS checks
    #[arg(short = 'k', long)]
    insecure: bool,

    /// Output directory
    #[arg(short, long, default_value = "dump")]
    output_dir: String,

    #[command(flatten)]
    verbose: Verbosity<InfoLevel>,
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();
    env_logger::builder()
        .format_target(false)
        .format_timestamp(None)
        .filter_level(args.verbose.log_level_filter())
        .target(env_logger::Target::Stdout)
        .init();

    log::trace!("{:?}", &args);

    let client = build_client(&args)?;
    let whoami = whoami(&client, &args).await?;
    let systemuser =
        get_entity::<dynamics::SystemUser>(&client, &args, "systemusers", &whoami.user_id).await?;
    log::info!(
        "systemuser [windowsliveid={}, systemuserid={}, title={}]",
        systemuser.windows_live_id,
        systemuser.system_user_id,
        systemuser.title
    );

    let userprivs =
        retrieve_systemuser_privileges(&client, &args, &systemuser.system_user_id).await?;

    for privilege in userprivs.role_privileges.iter() {
        log::info!(
            "roleprivilege [name={}, privilegeid={}]",
            privilege.privilege_name,
            privilege.privilege_id
        );
    }

    let entity_definitions =
        get_entity_set::<dynamics::EntityDefinition>(&client, &args, "EntityDefinitions").await?;

    for entity in entity_definitions.value.iter() {
        let result = get_entity_set::<HashMap<String, serde_json::Value>>(
            &client,
            &args,
            &entity.entity_set_name,
        )
        .await;

        if let Err(e) = &result {
            log::warn!("entityset failed {} with {}", &entity.entity_set_name, e);
        }

        if let Ok(r) = &result {
            log::info!(
                "dumped entityset {} [count={}]",
                &entity.entity_set_name,
                r.odata_count
            );

            if r.odata_count > 0 {
                let record_id = r.value[0]
                    .get(&entity.primary_id_attribute)
                    .unwrap()
                    .as_str()
                    .unwrap();

                let access_result = get_record_access_info(
                    &client,
                    &args,
                    &entity.logical_name,
                    &record_id,
                    &systemuser.system_user_id,
                )
                .await;

                if let Ok(outer) = access_result {
                    let inner: InnerAcessInfo = serde_json::from_str(&outer.access_info)?;
                    log::info!(
                        "recordprivilege {} [{}]",
                        &entity.entity_set_name,
                        &inner.granted_access_rights
                    );
                }
            }
        }
    }

    Ok(())
}

fn build_client(args: &Args) -> Result<reqwest::Client> {
    let mut builder = reqwest::Client::builder().default_headers(parse_headers(&args.headers)?);

    if args.insecure {
        builder = builder.danger_accept_invalid_certs(true);
    }

    if let Some(proxy) = args.proxy.clone() {
        builder = builder.proxy(reqwest::Proxy::all(proxy.clone())?);
    }

    Ok(builder.build()?)
}

async fn whoami(client: &reqwest::Client, args: &Args) -> Result<dynamics::WhoAmIResponse> {
    let response = client
        .get(args.target.to_owned() + API_ENDPOINT + &args.api + "/WhoAmI")
        .send()
        .await?
        .json::<dynamics::WhoAmIResponse>()
        .await?;

    Ok(response)
}

async fn retrieve_systemuser_privileges(
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
        .json::<dynamics::UserPrivileges>()
        .await?;

    Ok(response)
}

async fn get_entity<T: DeserializeOwned>(
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

async fn get_entity_set<T: DeserializeOwned + Serialize>(
    client: &reqwest::Client,
    args: &Args,
    entity_set_name: &str,
) -> Result<dynamics::EntitySet<T>> {
    let response = client
        .get(
            args.target.to_owned()
                + API_ENDPOINT
                + &args.api
                + "/"
                + entity_set_name
                + "?$count=true",
        )
        // .header(
        //     "Prefer",
        //     "odata.maxpagesize=1000,odata.include-annotations=\"Microsoft.Dynamics.CRM.totalrecordcountlimitexceeded\"",
        // )
        .send()
        .await?;

    if response.status() != 200 {
        return Err(anyhow!("request failed {}", response.status()));
    };

    fs::create_dir_all(&args.output_dir)?;

    let mut file_path = PathBuf::from(&args.output_dir);
    file_path.push(entity_set_name);
    file_path.set_extension("json");

    let mut write_file = File::create(&file_path)?;
    write_file.write_all(&response.bytes().await?)?;

    let read_file = File::open(&file_path)?;
    let json: EntitySet<T> = serde_json::from_reader(read_file)?;
    Ok(json)
}

async fn get_record_access_info(
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
        .json::<OuterAcessInfo>()
        .await?;

    Ok(response)
}
fn parse_headers(headers: &Vec<String>) -> Result<HeaderMap> {
    let mut header_map = HeaderMap::new();
    for header_str in headers.iter() {
        let split: Vec<&str> = header_str.split(':').collect();

        let name = split.get(0).unwrap(); //TODO
        let value = split.get(1).unwrap();

        let header_name = HeaderName::from_lowercase(name.to_lowercase().as_bytes())?;
        header_map.insert(header_name, value.parse()?);
    }

    Ok(header_map)
}
