use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Serialize, Deserialize, Debug)]
#[serde(rename_all = "PascalCase")]
pub struct WhoAmIResponse {
    #[serde(rename = "@odata.context")]
    pub odata_context: String,
    #[serde(rename = "UserId")]
    pub user_id: String,
    pub business_unit_id: String,
    pub organization_id: String,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct EntitySet<T> {
    #[serde(rename = "@odata.context")]
    pub odata_context: String,
    #[serde(rename = "@odata.count")]
    pub odata_count: i64,
    #[serde(rename = "@odata.nextLink")]
    pub odata_next: Option<String>,
    pub value: Vec<T>,
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(rename_all = "PascalCase")]
pub struct EntityDefinition {
    pub schema_name: String,
    pub logical_name: String,
    pub entity_set_name: String,
    pub primary_id_attribute: String,
    #[serde(flatten)]
    pub dynamic: HashMap<String, serde_json::Value>,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct SystemUser {
    #[serde(rename = "@odata.context")]
    pub odata_context: String,
    #[serde(rename = "windowsliveid")]
    pub windows_live_id: String,
    #[serde(rename = "systemuserid")]
    pub system_user_id: String,
    pub title: Option<String>,
    #[serde(flatten)]
    dynamic: HashMap<String, serde_json::Value>,
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(rename_all = "PascalCase")]
pub struct UserPrivileges {
    #[serde(rename = "@odata.context")]
    pub odata_context: String,
    pub role_privileges: Vec<RolePrivilege>,
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(rename_all = "PascalCase")]
pub struct RolePrivilege {
    pub depth: String,
    pub privilege_id: String,
    pub business_unit_id: String,
    pub privilege_name: String,
    pub record_filter_id: String,
    pub record_filter_unique_name: String,
}

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Root {
    pub error: Error,
}

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Error {
    pub code: String,
    pub message: String,
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(rename_all = "PascalCase")]
pub struct OuterAcessInfo {
    #[serde(rename = "@odata.context")]
    pub odata_context: String,
    pub access_info: String,
}

#[derive(Serialize, Deserialize, Debug)]
#[serde(rename_all = "PascalCase")]
pub struct InnerAcessInfo {
    pub granted_access_rights: String,
}
