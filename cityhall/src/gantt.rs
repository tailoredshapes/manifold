//! Mermaid Gantt rendering for a `ComputedPlan`.
//!
//! Determinism contract: same input → byte-identical output.

use crate::plan::ComputedPlan;
use std::fmt::Write;

/// Render a Mermaid Gantt chart for the given plan.
pub fn render_gantt(plan: &ComputedPlan) -> String {
    let mut out = String::new();
    let _ = writeln!(out, "gantt");
    let _ = writeln!(
        out,
        "    title ChangeRequest {} — {}",
        plan.change_request_summary, plan.tier
    );
    let _ = writeln!(out, "    dateFormat HH:mm");
    let _ = writeln!(out, "    axisFormat %H:%M");

    // Group steps by deployable while preserving the order each deployable
    // first appears in `plan.steps` — the planner has already topo-sorted, so
    // section order naturally matches deployment order.
    let mut section_for_step: Vec<usize> = Vec::with_capacity(plan.steps.len());
    let mut sections: Vec<(String, Vec<usize>)> = Vec::new();
    for (i, step) in plan.steps.iter().enumerate() {
        let key = &step.deployable_name;
        let pos = sections.iter().position(|(n, _)| n == key);
        let section_idx = match pos {
            Some(idx) => idx,
            None => {
                sections.push((key.clone(), Vec::new()));
                sections.len() - 1
            }
        };
        sections[section_idx].1.push(i);
        section_for_step.push(section_idx);
    }

    for (section_name, step_indices) in &sections {
        let _ = writeln!(out, "    section {section_name}");
        for &step_idx in step_indices {
            let step = &plan.steps[step_idx];

            // Emit each gate as a milestone first. Gate ids are deterministic:
            // gate_<step.order>_<gate_idx>.
            let mut gate_ids: Vec<String> = Vec::with_capacity(step.gates.len());
            for (gi, gate) in step.gates.iter().enumerate() {
                let gate_id = format!("gate_{}_{}", step.order, gi);
                let after = format_predecessors(step.order, &step.predecessor_orders);
                let _ = writeln!(
                    out,
                    "    {gt} ({src}) :crit, milestone, {gate_id}, {after}, 0min",
                    gt = gate.gate_type,
                    src = gate.source_org_node,
                );
                gate_ids.push(gate_id);
            }

            // Now the step itself: depends on either the gate(s) we just emitted,
            // or directly on its predecessors if no gates.
            let after = if !gate_ids.is_empty() {
                format!("after {}", gate_ids.join(" "))
            } else {
                format_predecessors(step.order, &step.predecessor_orders)
            };
            let _ = writeln!(
                out,
                "    {action} {name} :step_{order}, {after}, {mins}min",
                action = step.action,
                name = step.deployable_name,
                order = step.order,
                mins = step.estimated_minutes,
            );
        }
    }

    out
}

fn format_predecessors(current_order: usize, orders: &[usize]) -> String {
    // Filter out forward references. The planner emits the underlying deployable
    // graph faithfully, including cycles, and surfaces those cycles in
    // `ComputedPlan::blockers`. But a Mermaid Gantt cannot resolve a task that
    // depends on another task defined later — it computes NaN coordinates and
    // the chart fails to render. Predecessors with `order >= current_order` are
    // necessarily forward refs (the planner topo-sorts what it can; the residue
    // is the cycle), and are dropped here. The dependency relationship still
    // exists in the data model and in the blocker list; it just doesn't
    // contribute an edge to the Gantt.
    let preds: Vec<String> = orders
        .iter()
        .copied()
        .filter(|&o| o < current_order)
        .map(|o| format!("step_{o}"))
        .collect();
    if preds.is_empty() {
        // Mermaid task lines need a start anchor; use a fixed origin.
        "after start".to_string()
    } else {
        format!("after {}", preds.join(" "))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::plan::{ComputedPlan, PlanGate, PlanStep};

    fn step(
        order: usize,
        deployable_name: &str,
        preds: Vec<usize>,
        gates: Vec<PlanGate>,
    ) -> PlanStep {
        PlanStep {
            order,
            deployable_id: format!("dep-{order}"),
            deployable_name: deployable_name.to_string(),
            action: "deploy".to_string(),
            predecessor_orders: preds,
            gates,
            estimated_minutes: 10,
        }
    }

    fn gate(gate_type: &str, source: &str) -> PlanGate {
        PlanGate {
            gate_type: gate_type.to_string(),
            source_org_node: source.to_string(),
            description: None,
            window: None,
            approvers: None,
            quiesce_for: None,
        }
    }

    fn plan_with(steps: Vec<PlanStep>) -> ComputedPlan {
        ComputedPlan {
            change_request_id: "cr-1".to_string(),
            change_request_summary: "ship it".to_string(),
            tier: "prod".to_string(),
            steps,
            blockers: Vec::new(),
            computed_at: "2026-04-29T00:00:00Z".to_string(),
        }
    }

    #[test]
    fn one_step_no_gates_no_preds() {
        let p = plan_with(vec![step(0, "checkout", vec![], vec![])]);
        let g = render_gantt(&p);
        assert!(g.contains("section checkout"), "missing section: {g}");
        assert!(g.contains("deploy checkout :step_0, after start, 10min"), "got: {g}");
        assert!(g.contains("dateFormat HH:mm"));
    }

    #[test]
    fn two_steps_same_section_with_predecessor() {
        let p = plan_with(vec![
            step(0, "checkout", vec![], vec![]),
            step(1, "checkout", vec![0], vec![]),
        ]);
        let g = render_gantt(&p);
        assert!(g.contains("step_0, after start"));
        assert!(g.contains("step_1, after step_0"));
        // Only one section header
        assert_eq!(g.matches("section checkout").count(), 1, "got: {g}");
    }

    #[test]
    fn two_steps_in_different_sections() {
        let p = plan_with(vec![
            step(0, "auth", vec![], vec![]),
            step(1, "checkout", vec![0], vec![]),
        ]);
        let g = render_gantt(&p);
        assert!(g.contains("section auth"));
        assert!(g.contains("section checkout"));
        let auth_pos = g.find("section auth").unwrap();
        let checkout_pos = g.find("section checkout").unwrap();
        assert!(auth_pos < checkout_pos, "auth must come before checkout: {g}");
    }

    #[test]
    fn step_with_two_gates_renders_both_milestones_first() {
        let p = plan_with(vec![step(
            0,
            "billing",
            vec![],
            vec![
                gate("ApprovalGate", "Acme"),
                gate("WindowGate", "Payments-Domain"),
            ],
        )]);
        let g = render_gantt(&p);
        assert!(g.contains("ApprovalGate (Acme) :crit, milestone, gate_0_0,"));
        assert!(g.contains("WindowGate (Payments-Domain) :crit, milestone, gate_0_1,"));
        // Step depends on both gates.
        assert!(g.contains("deploy billing :step_0, after gate_0_0 gate_0_1, 10min"), "got: {g}");
    }

    #[test]
    fn forward_refs_from_cycles_are_dropped() {
        // Simulate a step that the planner couldn't fully topo-sort: it has a
        // predecessor with a higher order than itself (a cycle). The Gantt
        // should drop the forward ref to keep Mermaid happy; the cycle is
        // still reported via ComputedPlan::blockers elsewhere.
        let p = plan_with(vec![
            step(0, "auth", vec![5], vec![]),       // forward ref to step_5
            step(1, "checkout", vec![0, 3], vec![]), // step_3 is forward
            step(2, "billing", vec![1], vec![]),    // backward, fine
            step(3, "ledger", vec![2], vec![]),     // backward, fine
            step(5, "audit", vec![0], vec![]),      // backward, fine
        ]);
        let g = render_gantt(&p);
        // step_0 had only forward refs → falls back to `after start`
        assert!(g.contains("deploy auth :step_0, after start, 10min"), "got: {g}");
        // step_1 had one forward (3) and one backward (0) → backward kept
        assert!(g.contains("deploy checkout :step_1, after step_0, 10min"), "got: {g}");
        // step_5's only predecessor (0) is backward → kept as-is
        assert!(g.contains("deploy audit :step_5, after step_0, 10min"), "got: {g}");
        // No literal "after step_5" anywhere — that was the forward ref
        assert!(!g.contains("after step_5"), "forward ref leaked: {g}");
    }

    #[test]
    fn determinism() {
        let p = plan_with(vec![
            step(0, "auth", vec![], vec![gate("ApprovalGate", "Acme")]),
            step(1, "checkout", vec![0], vec![]),
        ]);
        assert_eq!(render_gantt(&p), render_gantt(&p));
    }
}
