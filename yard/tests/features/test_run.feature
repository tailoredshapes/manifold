Feature: Test runs

  Background:
    Given a Yard server is running
    And I have registered test_environment "checkout-iso" with kind "isolated"

  Scenario: Record a passing test run
    When I POST to "/test_run/api" with body {"test_environment_id": "<ids.checkout-iso>", "status": "passed", "duration_minutes": "12", "cost_actual": "0.42"}
    Then the response status should be 201
    And the response body should contain "passed"

  Scenario: Cannot record a run without a test_environment
    When I POST to "/test_run/api" with body {"status": "passed", "duration_minutes": "10"}
    Then the response status should be 400

  Scenario: Cannot record a run with an unknown status
    When I POST to "/test_run/api" with body {"test_environment_id": "<ids.checkout-iso>", "status": "vibing"}
    Then the response status should be 400

  Scenario: Filter runs by status via GraphQL
    Given I have recorded a passed test_run on "checkout-iso" with duration "12"
    And I have recorded a failed test_run on "checkout-iso" with duration "20"
    When I query the "test_run" graph with: { getByStatus(status: "passed") { duration_minutes } }
    Then there should be no GraphQL errors
    And the response data should contain "12"

  Scenario: History aggregates duration and pass-rate
    Given I have recorded a passed test_run on "checkout-iso" with duration "10"
    And I have recorded a passed test_run on "checkout-iso" with duration "30"
    And I have recorded a failed test_run on "checkout-iso" with duration "20"
    When I GET "/test_environment/<ids.checkout-iso>/history"
    Then the response status should be 200
    And the response body should contain "run_count"
    And the response body should contain "average_duration_minutes"
    And the history pass_rate should be 0.6666666666666666
    And the history average_duration_minutes should be 20

  Scenario: Federated change_request resolves through Cityhall
    Given the Cityhall stub knows change request "cr-deploy-v2-uuid" with summary "deploy v2"
    When I POST to "/test_run/api" with body {"test_environment_id": "<ids.checkout-iso>", "change_request_id": "cr-deploy-v2-uuid", "status": "passed"}
    Then the response status should be 201
    Given I capture the last id as "tr"
    When I query the "test_run" graph with: { getById(id: "<ids.tr>") { test_environment_id change_request { id summary } } }
    Then there should be no GraphQL errors
    And the response data should contain "deploy v2"
