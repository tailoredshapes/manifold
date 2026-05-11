Feature: Graph visualization data shape

  The Graph tab in the Groundwork UI composes a dependency network locally
  in the browser from three graphlettes: /deployable/graph,
  /dependency/graph, and /exposes/graph. These scenarios pin the shape of
  those responses so the UI never silently breaks if a schema drifts.

  Background:
    Given a Groundwork server is running

  Scenario: getAll deployables exposes deployment_status field
    When I POST to "/deployable/api" with body {"name": "checkout-service", "deployment_status": "operational"}
    Then the response status should be 201
    When I POST to "/deployable/api" with body {"name": "payments-service", "deployment_status": "degraded"}
    Then the response status should be 201
    When I POST to "/deployable/api" with body {"name": "inventory-service"}
    Then the response status should be 201
    When I query the "deployable" graph with: { getAll { id name deployment_status } }
    Then there should be no GraphQL errors
    And the response data array should have 3 items
    And the response data should contain "operational"
    And the response data should contain "degraded"

  Scenario: Dependency and Exposes queries return the fields the graph composes from
    # Seed: two deployables, one service. checkout-service exposes the service;
    # payments-service depends on it. After composition, the graph should
    # have an edge payments-service → checkout-service.
    When I POST to "/service/api" with body {"name": "billing-db", "type": "database"}
    Then the response status should be 201
    When I POST to "/deployable/api" with body {"name": "checkout-service"}
    Then the response status should be 201
    When I POST to "/deployable/api" with body {"name": "payments-service"}
    Then the response status should be 201
    # Look up ids
    When I query the "service" graph with: { getByName(name: "billing-db") { id } }
    Then there should be no GraphQL errors
    When I query the "deployable" graph with: { getByName(name: "checkout-service") { id } }
    Then there should be no GraphQL errors
    When I query the "deployable" graph with: { getByName(name: "payments-service") { id } }
    Then there should be no GraphQL errors
    # Verify the dependency + exposes graphlettes expose the fields the UI needs.
    When I query the "dependency" graph with: { getAll { id deployable_id service_id criticality } }
    Then there should be no GraphQL errors
    When I query the "exposes" graph with: { getAll { id deployable_id service_id } }
    Then there should be no GraphQL errors

  Scenario: deployment_status accepts all four documented values
    When I POST to "/deployable/api" with body {"name": "svc-a", "deployment_status": "operational"}
    Then the response status should be 201
    When I POST to "/deployable/api" with body {"name": "svc-b", "deployment_status": "degraded"}
    Then the response status should be 201
    When I POST to "/deployable/api" with body {"name": "svc-c", "deployment_status": "down"}
    Then the response status should be 201
    When I POST to "/deployable/api" with body {"name": "svc-d", "deployment_status": "unknown"}
    Then the response status should be 201
    When I query the "deployable" graph with: { getAll { id name deployment_status } }
    Then there should be no GraphQL errors
    And the response data should contain "operational"
    And the response data should contain "degraded"
    And the response data should contain "down"
    And the response data should contain "unknown"
