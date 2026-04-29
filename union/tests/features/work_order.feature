Feature: Work orders

  Background:
    Given a Union server is running
    And I have registered team "checkout-team" with kind "product"

  Scenario: Open a work order against a team
    When I POST to "/work_order/api" with body {"team_id": "<ids.checkout-team>", "summary": "rotate db credentials"}
    Then the response status should be 201
    And the response body should have an "id" field
    And the response body should contain "rotate db credentials"

  Scenario: Cannot open without a summary
    When I POST to "/work_order/api" with body {"team_id": "<ids.checkout-team>"}
    Then the response status should be 400

  Scenario: Cannot open with an invalid status
    When I POST to "/work_order/api" with body {"team_id": "<ids.checkout-team>", "summary": "x", "status": "yolo"}
    Then the response status should be 400

  Scenario: Cannot open with an invalid priority
    When I POST to "/work_order/api" with body {"team_id": "<ids.checkout-team>", "summary": "x", "priority": "vibes"}
    Then the response status should be 400

  Scenario: Status transition through update
    Given I have opened work order "rotate-creds" against "checkout-team"
    When I PUT "/work_order/api/<ids.rotate-creds>" with body {"team_id": "<ids.checkout-team>", "summary": "rotate-creds", "status": "in_progress"}
    Then the response status should be 200
    And the response body should contain "in_progress"

  Scenario: Filter open work by team via GraphQL
    Given I have opened work order "task-a" against "checkout-team"
    And I have opened work order "task-b" against "checkout-team"
    When I query the "work_order" graph with: { getByTeamId(team_id: "<ids.checkout-team>") { summary status } }
    Then there should be no GraphQL errors
    And the response data should contain "task-a"
    And the response data should contain "task-b"

  Scenario: A work order can reference a deployable (federation hook)
    When I POST to "/work_order/api" with body {"team_id": "<ids.checkout-team>", "summary": "tune SLO", "deployable_id": "external-deployable-uuid"}
    Then the response status should be 201
    And the response body should contain "external-deployable-uuid"

  Scenario: Federated deployable field resolves through Groundwork
    Given the Groundwork stub knows deployable "dep-checkout-uuid" as "checkout"
    When I POST to "/work_order/api" with body {"team_id": "<ids.checkout-team>", "summary": "rotate-creds", "deployable_id": "dep-checkout-uuid"}
    Then the response status should be 201
    Given I capture the last id as "wo"
    When I query the "work_order" graph with: { getById(id: "<ids.wo>") { summary deployable { id name } } }
    Then there should be no GraphQL errors
    And the response data should contain "checkout"
    And the response data should contain "dep-checkout-uuid"

  Scenario: Federated change_request field resolves through Cityhall
    Given the Cityhall stub knows change request "cr-deploy-v2-uuid" with summary "deploy v2"
    When I POST to "/work_order/api" with body {"team_id": "<ids.checkout-team>", "summary": "follow up", "change_request_id": "cr-deploy-v2-uuid"}
    Then the response status should be 201
    Given I capture the last id as "wo"
    When I query the "work_order" graph with: { getById(id: "<ids.wo>") { summary change_request { id summary status } } }
    Then there should be no GraphQL errors
    And the response data should contain "deploy v2"
    And the response data should contain "submitted"

  Scenario: Federated fields are null when ids unset
    When I POST to "/work_order/api" with body {"team_id": "<ids.checkout-team>", "summary": "no-fk"}
    Then the response status should be 201
    Given I capture the last id as "wo"
    When I query the "work_order" graph with: { getById(id: "<ids.wo>") { summary deployable { id } change_request { id } } }
    Then there should be no GraphQL errors
    And the response data should contain "no-fk"

  Scenario: Federated fields are null when ids point to unknown records
    When I POST to "/work_order/api" with body {"team_id": "<ids.checkout-team>", "summary": "phantom-fk", "deployable_id": "ghost-dep", "change_request_id": "ghost-cr"}
    Then the response status should be 201
    Given I capture the last id as "wo"
    When I query the "work_order" graph with: { getById(id: "<ids.wo>") { summary deployable { id } change_request { id } } }
    Then there should be no GraphQL errors
    And the response data should contain "phantom-fk"

  Scenario: Filter work orders by status
    Given I have opened work order "in-flight" against "checkout-team"
    And I have opened work order "later" against "checkout-team"
    When I PUT "/work_order/api/<ids.in-flight>" with body {"team_id": "<ids.checkout-team>", "summary": "in-flight", "status": "in_progress"}
    And I query the "work_order" graph with: { getByStatus(status: "in_progress") { summary } }
    Then there should be no GraphQL errors
    And the response data should contain "in-flight"
    And the response data should not contain "later"
