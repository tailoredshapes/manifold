Feature: Bylaws layered along the org chart

  Background:
    Given a Cityhall server is running
    And I have built the standard hierarchy

  Scenario: Attach an enterprise-level freeze
    When I POST to "/bylaw/api" with body {"org_node_id": "<ids.acme>", "gate_type": "FreezePeriod", "window": "2026-12-23T00:00Z/2026-12-27T00:00Z", "description": "year-end freeze", "priority": "100"}
    Then the response status should be 201
    And the response body should contain "FreezePeriod"

  Scenario: Cannot attach with unknown gate_type
    When I POST to "/bylaw/api" with body {"org_node_id": "<ids.acme>", "gate_type": "VibesCheck"}
    Then the response status should be 400

  Scenario: WindowGate requires a window
    When I POST to "/bylaw/api" with body {"org_node_id": "<ids.acme>", "gate_type": "WindowGate"}
    Then the response status should be 400

  Scenario: ApprovalGate requires approvers
    When I POST to "/bylaw/api" with body {"org_node_id": "<ids.acme>", "gate_type": "ApprovalGate"}
    Then the response status should be 400

  Scenario: QuiesceGate requires quiesce_for
    When I POST to "/bylaw/api" with body {"org_node_id": "<ids.acme>", "gate_type": "QuiesceGate"}
    Then the response status should be 400

  Scenario: Effective bylaws for a leaf walk all ancestors root-first
    Given enterprise "<ids.acme>" has a "FreezePeriod" bylaw with window "2026-12-23T00:00Z/2026-12-27T00:00Z"
    When I POST to "/bylaw/api" with body {"org_node_id": "<ids.payments>", "gate_type": "ApprovalGate", "approvers": "person-abc", "priority": "50"}
    And I POST to "/bylaw/api" with body {"org_node_id": "<ids.checkout>", "gate_type": "QuiesceGate", "quiesce_for": "15m", "priority": "10"}
    And I GET "/org_node/<ids.checkout>/effective_bylaws"
    Then the response status should be 200
    And the response body should contain "FreezePeriod"
    And the response body should contain "ApprovalGate"
    And the response body should contain "QuiesceGate"

  Scenario: A child cannot loosen a parent bylaw — it stays effective
    Given enterprise "<ids.acme>" has a "FreezePeriod" bylaw with window "2026-12-23T00:00Z/2026-12-27T00:00Z"
    When I POST to "/bylaw/api" with body {"org_node_id": "<ids.checkout>", "gate_type": "AutoGate", "priority": "100"}
    And I GET "/org_node/<ids.checkout>/effective_bylaws"
    Then the response body should contain "FreezePeriod"
    And the response body should contain "AutoGate"
