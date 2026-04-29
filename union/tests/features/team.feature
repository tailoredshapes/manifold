Feature: Team CRUD

  Background:
    Given a Union server is running

  Scenario: Register a product team
    When I POST to "/team/api" with body {"name": "checkout-team", "kind": "product"}
    Then the response status should be 201
    And the response body should contain "checkout-team"
    And the response body should contain "product"

  Scenario: Cannot register a team without a kind
    When I POST to "/team/api" with body {"name": "orphan"}
    Then the response status should be 400

  Scenario: Cannot register a team with an invalid kind
    When I POST to "/team/api" with body {"name": "weird", "kind": "wizards"}
    Then the response status should be 400

  Scenario: Find teams by kind via GraphQL
    Given I have registered team "checkout-team" with kind "product"
    And I have registered team "platform-eng" with kind "platform"
    And I have registered team "appsec" with kind "security"
    When I query the "team" graph with: { getByKind(kind: "product") { name } }
    Then there should be no GraphQL errors
    And the response data should contain "checkout-team"
    And the response data should not contain "platform-eng"

  Scenario: Update team description
    Given I have registered team "billing-team" with kind "product"
    When I PUT "/team/api/<ids.billing-team>" with body {"name": "billing-team", "kind": "product", "description": "owns invoicing + payments rail"}
    Then the response status should be 200
    And the response body should contain "owns invoicing + payments rail"
