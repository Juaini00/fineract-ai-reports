use super::*;

#[test]
fn capability_option_label_uses_requested_period_not_catalog_example() {
    let capability = CapabilityKnowledge {
        id: "savings_deposit_total".to_string(),
        status: "approved_mvp".to_string(),
        domain: "savings".to_string(),
        query_id: "savings.deposit_total".to_string(),
        output_mode: "total".to_string(),
        display_name: None,
        description: None,
        data_areas: Vec::new(),
        metrics: Vec::new(),
        examples: vec!["What is the total deposit this month?".to_string()],
        required_parameters: Vec::new(),
        optional_parameters: Vec::new(),
    };

    assert_eq!(
        capability_option_label(&capability, "Show customer savings activity this week"),
        "Total deposit this week"
    );
}
