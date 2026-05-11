Feature: Cityhall MCP server

  Background:
    Given a Cityhall server is running
    And the MCP binary is started against the server
    And I have built the standard hierarchy
    And I have registered enterprise bylaw "freeze" of type "FreezePeriod" with window "fri-mon"
    And I have registered enterprise bylaw "approve" of type "ApprovalGate" with approvers "ceo"
    And I have registered division bylaw "windowed" of type "WindowGate" with window "weekdays"
    And I have registered division bylaw "quiet" of type "QuiesceGate" with quiesce_for "1h"
    And I have registered domain bylaw "auto" of type "AutoGate"
    And I have registered team bylaw "team-approve" of type "ApprovalGate" with approvers "tl"
    And I have submitted change request "ship-checkout" targeting "dep-checkout"

  Scenario: tools/list returns the auto-derived catalogue + custom capabilities
    When I send MCP request "tools/list"
    Then the response should include tool "list_org_nodes"
    And the response should include tool "list_bylaws"
    And the response should include tool "get_change_request_by_id"
    And the response should include tool "ancestors_of_org_node"
    And the response should include tool "effective_bylaws_for_org_node"
    And the response should include tool "compute_plan_for_change_request"
    And the response should include tool "render_gantt_for_plan"

  Scenario: list_bylaws returns seeded bylaws
    When I call MCP tool "list_bylaws" with arguments {}
    Then the tool result should be a JSON array of at least 6 records

  Scenario: ancestors_of_org_node returns the chain to a leaf node
    When I call MCP tool "ancestors_of_org_node" with arguments {"org_node_id": "<ids.checkout>"}
    Then the tool result names should be in order "Acme,Engineering,Payments,Checkout Team"

  Scenario: compute_plan_for_change_request returns a populated plan envelope
    When I call MCP tool "compute_plan_for_change_request" with arguments {"change_request_id": "<ids.ship-checkout>", "tier": "prod"}
    Then the tool result should have plan steps and blockers populated
