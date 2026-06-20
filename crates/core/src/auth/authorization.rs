use std::{collections::HashSet, fmt};

use crate::auth::model::ClientContext;

#[derive(Debug, PartialEq, Eq)]
pub enum AuthorizationError {
    CapabilityNotAllowed(String),

    MissingOfficeScope,

    OfficeNotAllowed(i64),

    PiiNotAllowed,
}

impl fmt::Display for AuthorizationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::CapabilityNotAllowed(capability) => {
                write!(formatter, "API key is not allowed to run capability `{capability}`")
            }
            Self::MissingOfficeScope => write!(formatter, "API key has no office scope"),
            Self::OfficeNotAllowed(office_id) => {
                write!(formatter, "requested office `{office_id}` is outside API key scope")
            }
            Self::PiiNotAllowed => write!(formatter, "PII output is not allowed for this API key"),
        }
    }
}

impl std::error::Error for AuthorizationError {}

pub fn ensure_capability_allowed(
    client: &ClientContext,
    capability: &str,
) -> Result<(), AuthorizationError> {
    if client
        .allowed_capabilities
        .iter()
        .any(|allowed| allowed == capability)
    {
        Ok(())
    } else {
        Err(AuthorizationError::CapabilityNotAllowed(
            capability.to_string(),
        ))
    }
}

pub fn effective_office_scope(
    client: &ClientContext,
    requested_office_ids: Option<&[i64]>,
) -> Result<Vec<i64>, AuthorizationError> {
    if client.allowed_office_ids.is_empty() {
        return Err(AuthorizationError::MissingOfficeScope);
    }

    let allowed: HashSet<i64> = client.allowed_office_ids.iter().copied().collect();

    let office_ids = match requested_office_ids {
        Some(requested) => {
            for office_id in requested {
                if !allowed.contains(office_id) {
                    return Err(AuthorizationError::OfficeNotAllowed(*office_id));
                }
            }

            requested.to_vec()
        }
        None => client.allowed_office_ids.clone(),
    };

    Ok(office_ids)
}

pub fn ensure_pii_allowed(
    client: &ClientContext,
    output_requires_pii: bool,
) -> Result<(), AuthorizationError> {
    if !output_requires_pii || client.can_view_pii {
        Ok(())
    } else {
        Err(AuthorizationError::PiiNotAllowed)
    }
}

pub fn pii_output_allowed(client: &ClientContext, output_requires_pii: bool) -> bool {
    ensure_pii_allowed(client, output_requires_pii).is_ok()
}

// TODO(reporting): call these guards from the reporting execution plan before any
// Fineract SQL is executed, then select only fields allowed by the capability and
// PII policy. Office filtering must happen inside approved SQL, not after fetching.

#[cfg(test)]
mod tests {
    use uuid::Uuid;

    use super::*;

    fn client() -> ClientContext {
        ClientContext {
            api_key_id: Uuid::nil(),
            name: "test-client".to_string(),
            owner: "test-owner".to_string(),
            key_prefix: "air_test".to_string(),
            allowed_office_ids: vec![1, 2],
            allowed_capabilities: vec!["savings_deposit_total".to_string()],
            can_view_pii: false,
            expires_at: None,
        }
    }

    #[test]
    fn allows_configured_capability() {
        let client = client();

        assert!(ensure_capability_allowed(&client, "savings_deposit_total").is_ok());
    }

    #[test]
    fn rejects_unconfigured_capability() {
        let client = client();

        assert_eq!(
            ensure_capability_allowed(&client, "savings_deposit_top_n"),
            Err(AuthorizationError::CapabilityNotAllowed(
                "savings_deposit_top_n".to_string()
            ))
        );
    }

    #[test]
    fn uses_all_allowed_offices_when_request_omits_scope() {
        let client = client();

        assert_eq!(effective_office_scope(&client, None), Ok(vec![1, 2]));
    }

    #[test]
    fn allows_requested_subset_of_offices() {
        let client = client();

        assert_eq!(effective_office_scope(&client, Some(&[2])), Ok(vec![2]));
    }

    #[test]
    fn rejects_office_outside_scope() {
        let client = client();

        assert_eq!(
            effective_office_scope(&client, Some(&[3])),
            Err(AuthorizationError::OfficeNotAllowed(3))
        );
    }

    #[test]
    fn rejects_empty_office_scope() {
        let mut client = client();
        client.allowed_office_ids.clear();

        assert_eq!(
            effective_office_scope(&client, None),
            Err(AuthorizationError::MissingOfficeScope)
        );
    }

    #[test]
    fn rejects_pii_when_api_key_cannot_view_pii() {
        let client = client();

        assert_eq!(
            ensure_pii_allowed(&client, true),
            Err(AuthorizationError::PiiNotAllowed)
        );
    }

    #[test]
    fn allows_pii_when_api_key_can_view_pii() {
        let mut client = client();
        client.can_view_pii = true;

        assert!(ensure_pii_allowed(&client, true).is_ok());
    }
}
