Feature: Application CRUD

  Background:
    Given a Groundwork server is running

  Scenario: Register an application with just a name
    When I POST to "/application/api" with body {"name": "checkout-service"}
    Then the response status should be 200
    And the response body should contain "checkout-service"
    And the response body should have an "id" field

  Scenario: Cannot register an application without a name
    When I POST to "/application/api" with body {}
    Then the response status should be 400

  Scenario: List all applications
    Given I have registered applications:
      | name              |
      | checkout-service  |
      | payment-service   |
      | identity-service  |
    When I GET "/application/api"
    Then the response status should be 200
    And the response body should be a JSON array
    And the response array should have 3 items

  Scenario: Retrieve an application by ID
    Given I have registered application "inventory-service"
    When I GET "/application/api/<ids.inventory-service>"
    Then the response status should be 200
    And the response body should contain "inventory-service"

  Scenario: Update an application's optional fields
    Given I have registered application "reporting-service"
    When I PUT "/application/api/<ids.reporting-service>" with body {"name": "reporting-service", "description": "Handles reports", "repo_url": "https://github.com/acme/reporting"}
    Then the response status should be 200
    And the response body should contain "Handles reports"

  Scenario: Delete an application
    Given I have registered application "temp-service"
    When I DELETE "/application/api/<ids.temp-service>"
    Then the response status should be 200
    When I GET "/application/api/<ids.temp-service>"
    Then the response status should be 404

  Scenario: Query application by name via GraphQL
    Given I have registered application "search-service"
    When I query the "application" graph with: { getByName(name: "search-service") { id name } }
    Then there should be no GraphQL errors
    And the response data should contain "search-service"

  Scenario: Temporal query returns application as it was
    Given I have registered application "versioned-service"
    And I capture the current timestamp as "before_update"
    And I update application "versioned-service" with body {"name": "versioned-service", "description": "v2 description"}
    When I query the "application" graph with: { getById(id: "<ids.versioned-service>", at: <timestamps.before_update>) { name description } }
    Then there should be no GraphQL errors
    And the response data description should be null
