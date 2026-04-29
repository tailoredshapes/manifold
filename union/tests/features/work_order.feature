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

  Scenario: Filter work orders by status
    Given I have opened work order "in-flight" against "checkout-team"
    And I have opened work order "later" against "checkout-team"
    When I PUT "/work_order/api/<ids.in-flight>" with body {"team_id": "<ids.checkout-team>", "summary": "in-flight", "status": "in_progress"}
    And I query the "work_order" graph with: { getByStatus(status: "in_progress") { summary } }
    Then there should be no GraphQL errors
    And the response data should contain "in-flight"
    And the response data should not contain "later"
