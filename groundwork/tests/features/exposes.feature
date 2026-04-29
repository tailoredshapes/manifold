Feature: Exposes relationship (Deployable exposes Service)

  Background:
    Given a Groundwork server is running
    And I have registered deployable "checkout"
    And I have registered service "checkout-api"

  Scenario: Record that a deployable exposes a service
    When I POST to "/exposes/api" with body {"deployable_id": "<ids.checkout>", "service_id": "<ids.checkout-api>"}
    Then the response status should be 201
    And the response body should have an "id" field

  Scenario: Cannot record exposes without deployable_id
    When I POST to "/exposes/api" with body {"service_id": "<ids.checkout-api>"}
    Then the response status should be 400

  Scenario: Cannot record exposes without service_id
    When I POST to "/exposes/api" with body {"deployable_id": "<ids.checkout>"}
    Then the response status should be 400

  Scenario: Record exposes with optional port and protocol
    When I POST to "/exposes/api" with body {"deployable_id": "<ids.checkout>", "service_id": "<ids.checkout-api>", "port": "8080", "protocol": "http"}
    Then the response status should be 201
    And the response body should contain "8080"
    And the response body should contain "http"

  Scenario: A service can be exposed by multiple deployables
    Given I have registered deployable "checkout-canary"
    When I POST to "/exposes/api" with body {"deployable_id": "<ids.checkout>", "service_id": "<ids.checkout-api>"}
    And I POST to "/exposes/api" with body {"deployable_id": "<ids.checkout-canary>", "service_id": "<ids.checkout-api>"}
    And I query the "exposes" graph with: { getByServiceId(service_id: "<ids.checkout-api>") { deployable_id } }
    Then there should be no GraphQL errors
    And the response data should contain "<ids.checkout>"
    And the response data should contain "<ids.checkout-canary>"

  Scenario: A service may exist with no deployables exposing it
    Given I have registered service "stripe"
    When I query the "exposes" graph with: { getByServiceId(service_id: "<ids.stripe>") { id } }
    Then there should be no GraphQL errors
    And the response data array should have 0 items

  Scenario: Find what a deployable exposes via GraphQL
    Given I have registered service "checkout-grpc"
    When I POST to "/exposes/api" with body {"deployable_id": "<ids.checkout>", "service_id": "<ids.checkout-api>", "port": "8080", "protocol": "http"}
    And I POST to "/exposes/api" with body {"deployable_id": "<ids.checkout>", "service_id": "<ids.checkout-grpc>", "port": "9090", "protocol": "grpc"}
    And I query the "exposes" graph with: { getByDeployableId(deployable_id: "<ids.checkout>") { service_id port protocol } }
    Then there should be no GraphQL errors
    And the response data should contain "<ids.checkout-api>"
    And the response data should contain "<ids.checkout-grpc>"
    And the response data should contain "9090"
