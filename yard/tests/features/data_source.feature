Feature: Data sources

  Background:
    Given a Yard server is running

  Scenario: Register a prod snapshot source
    When I POST to "/data_source/api" with body {"name": "checkout-prod-snapshot", "kind": "prod_snapshot", "location": "s3://snapshots/checkout"}
    Then the response status should be 201
    And the response body should contain "checkout-prod-snapshot"

  Scenario: Cannot register without a kind
    When I POST to "/data_source/api" with body {"name": "anonymous"}
    Then the response status should be 400

  Scenario: Cannot register with an unknown kind
    When I POST to "/data_source/api" with body {"name": "x", "kind": "telepathy"}
    Then the response status should be 400

  Scenario: Cannot register with an unknown refresh policy
    When I POST to "/data_source/api" with body {"name": "x", "kind": "synthetic", "refresh_policy": "moonphase"}
    Then the response status should be 400

  Scenario: Find data sources by kind
    Given I have registered data_source "snap-1" with kind "prod_snapshot"
    And I have registered data_source "syn-1" with kind "synthetic"
    When I query the "data_source" graph with: { getByKind(kind: "prod_snapshot") { name } }
    Then there should be no GraphQL errors
    And the response data should contain "snap-1"
    And the response data should not contain "syn-1"
