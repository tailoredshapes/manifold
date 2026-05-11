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

  Scenario: tools/list returns the catalogue + custom tools
    When I send MCP request "tools/list"
    Then the response should include tool "catalog.list"
    And the response should include tool "catalog.get"
    And the response should include tool "catalog.search"
    And the response should include tool "org.ancestors"
    And the response should include tool "org.effective_bylaws"
    And the response should include tool "change_request.plan"
    And the response should include tool "deployment_plan.gantt"

  Scenario: catalog.list returns bylaws
    When I call MCP tool "catalog.list" with arguments {"entity": "bylaw"}
    Then the tool result should be a JSON array of at least 6 records

  Scenario: org.ancestors returns the chain to a leaf node
    When I call MCP tool "org.ancestors" with arguments {"org_node_id": "<ids.checkout>"}
    Then the tool result names should be in order "Acme,Engineering,Payments,Checkout Team"

  Scenario: change_request.plan returns a populated plan envelope
    When I call MCP tool "change_request.plan" with arguments {"change_request_id": "<ids.ship-checkout>", "tier": "prod"}
    Then the tool result should have plan steps and blockers populated
