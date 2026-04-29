Feature: Dependency relationship (Deployable depends on Service)

  Background:
    Given a Groundwork server is running
    And I have registered deployable "checkout"
    And I have registered service "payments-api"

  Scenario: Register a dependency from a deployable to a service
    When I POST to "/dependency/api" with body {"deployable_id": "<ids.checkout>", "service_id": "<ids.payments-api>"}
    Then the response status should be 201
    And the response body should have an "id" field

  Scenario: Cannot register a dependency without deployable_id
    When I POST to "/dependency/api" with body {"service_id": "<ids.payments-api>"}
    Then the response status should be 400

  Scenario: Cannot register a dependency without service_id
    When I POST to "/dependency/api" with body {"deployable_id": "<ids.checkout>"}
    Then the response status should be 400

  Scenario: Find dependencies by deployable via GraphQL
    Given I have registered service "search-api"
    When I POST to "/dependency/api" with body {"deployable_id": "<ids.checkout>", "service_id": "<ids.payments-api>"}
    And I POST to "/dependency/api" with body {"deployable_id": "<ids.checkout>", "service_id": "<ids.search-api>"}
    And I query the "dependency" graph with: { getByDeployableId(deployable_id: "<ids.checkout>") { id service_id } }
    Then there should be no GraphQL errors
    And the response data should contain "<ids.payments-api>"
    And the response data should contain "<ids.search-api>"

  Scenario: Find dependents by service via GraphQL
    Given I have registered deployable "billing"
    When I POST to "/dependency/api" with body {"deployable_id": "<ids.checkout>", "service_id": "<ids.payments-api>"}
    And I POST to "/dependency/api" with body {"deployable_id": "<ids.billing>", "service_id": "<ids.payments-api>"}
    And I query the "dependency" graph with: { getByServiceId(service_id: "<ids.payments-api>") { id deployable_id } }
    Then there should be no GraphQL errors
    And the response data should contain "<ids.checkout>"
    And the response data should contain "<ids.billing>"
