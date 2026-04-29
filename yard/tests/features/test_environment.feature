Feature: Test environments

  Background:
    Given a Yard server is running

  Scenario: Register a sandbox env
    When I POST to "/test_environment/api" with body {"name": "checkout-sandbox", "kind": "sandbox", "spinup_minutes": "15", "cost_per_hour": "0.05"}
    Then the response status should be 201
    And the response body should contain "checkout-sandbox"
    And the response body should contain "sandbox"

  Scenario: Cannot register without a kind
    When I POST to "/test_environment/api" with body {"name": "orphan"}
    Then the response status should be 400

  Scenario: Cannot register with an unknown kind
    When I POST to "/test_environment/api" with body {"name": "weird", "kind": "vibesnet"}
    Then the response status should be 400

  Scenario: External env requires a contractual_limit
    When I POST to "/test_environment/api" with body {"name": "stripe-sandbox", "kind": "external"}
    Then the response status should be 400
    And the response body should contain "contractual_limit"

  Scenario: External env with contractual_limit succeeds
    When I POST to "/test_environment/api" with body {"name": "stripe-sandbox", "kind": "external", "contractual_limit": "5", "rate_limit": "10/min"}
    Then the response status should be 201
    And the response body should contain "stripe-sandbox"

  Scenario: Mock env requires a mock_source_id
    When I POST to "/test_environment/api" with body {"name": "auth-mock", "kind": "mock"}
    Then the response status should be 400
    And the response body should contain "mock_source_id"

  Scenario: Find envs by kind via GraphQL
    Given I have registered test_environment "alpha" with kind "sandbox"
    And I have registered test_environment "beta" with kind "isolated"
    When I query the "test_environment" graph with: { getByKind(kind: "sandbox") { name } }
    Then there should be no GraphQL errors
    And the response data should contain "alpha"
    And the response data should not contain "beta"

  Scenario: Federated deployable field resolves through Groundwork
    Given the Groundwork stub knows deployable "dep-checkout-uuid" as "checkout"
    When I POST to "/test_environment/api" with body {"name": "checkout-iso", "kind": "isolated", "deployable_id": "dep-checkout-uuid"}
    Then the response status should be 201
    Given I capture the last id as "env"
    When I query the "test_environment" graph with: { getById(id: "<ids.env>") { name deployable { id name } } }
    Then there should be no GraphQL errors
    And the response data should contain "checkout"
