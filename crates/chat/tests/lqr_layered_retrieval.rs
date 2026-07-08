#[test]
fn off_domain_prompt_short_circuits_at_layer_1() {
    let ranked = vec![
        ("loan".to_string(), "deferred".to_string(), 0.85_f32),
        ("savings".to_string(), "approved_mvp".to_string(), 0.55_f32),
    ];
    let policy = chat::knowledge::model::LqrPolicy::default();

    let decision = chat::chat::pipeline::lqr::decide_domain_layer(&policy, &ranked);

    match decision {
        chat::chat::pipeline::lqr::DomainDecision::Reject { reason } => {
            assert!(reason.contains("off_domain_loan"));
        }
        _ => panic!("expected off-domain reject"),
    }
}
