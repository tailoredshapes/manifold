Feature: Test suites

  Background:
    Given a Yard server is running

  Scenario: Register a test suite
    When I POST to "/test_suite/api" with body {"name": "checkout-integration", "runner": "cargo", "command": "cargo test -p checkout"}
    Then the response status should be 201
    And the response body should contain "checkout-integration"

  Scenario: Cannot register a suite without a name
    When I POST to "/test_suite/api" with body {"runner": "cargo"}
    Then the response status should be 400

  Scenario: Find suites by deployable id
    Given I have registered test_suite "alpha" against deployable "dep-checkout-uuid"
    And I have registered test_suite "beta" against deployable "dep-auth-uuid"
    When I query the "test_suite" graph with: { getByDeployableId(deployable_id: "dep-checkout-uuid") { name } }
    Then there should be no GraphQL errors
    And the response data should contain "alpha"
    And the response data should not contain "beta"
