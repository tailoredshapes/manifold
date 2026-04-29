Feature: Deployable CRUD

  Background:
    Given a Groundwork server is running

  Scenario: Register a deployable with just a name
    When I POST to "/deployable/api" with body {"name": "checkout-service"}
    Then the response status should be 201
    And the response body should contain "checkout-service"
    And the response body should have an "id" field

  Scenario: Cannot register a deployable without a name
    When I POST to "/deployable/api" with body {}
    Then the response status should be 400

  Scenario: List all deployables
    Given I have registered deployables:
      | name              |
      | checkout-service  |
      | payment-service   |
      | identity-service  |
    When I GET "/deployable/api"
    Then the response status should be 200
    And the response body should be a JSON array
    And the response array should have 3 items

  Scenario: Retrieve a deployable by ID
    Given I have registered deployable "inventory-service"
    When I GET "/deployable/api/<ids.inventory-service>"
    Then the response status should be 200
    And the response body should contain "inventory-service"

  Scenario: Update a deployable's optional fields
    Given I have registered deployable "reporting-service"
    When I PUT "/deployable/api/<ids.reporting-service>" with body {"name": "reporting-service", "description": "Handles reports", "repo_url": "https://github.com/acme/reporting"}
    Then the response status should be 200
    And the response body should contain "Handles reports"

  Scenario: Register a deployable with a team_id (federation key for Union)
    When I POST to "/deployable/api" with body {"name": "billing", "team_id": "team-abc-uuid"}
    Then the response status should be 201
    And the response body should contain "team-abc-uuid"
    When I query the "deployable" graph with: { getByName(name: "billing") { id name team_id } }
    Then there should be no GraphQL errors
    And the response data should contain "team-abc-uuid"

  Scenario: Delete a deployable
    Given I have registered deployable "temp-service"
    When I DELETE "/deployable/api/<ids.temp-service>"
    Then the response status should be 200
    When I GET "/deployable/api/<ids.temp-service>"
    Then the response status should be 404

  Scenario: Query deployable by name via GraphQL
    Given I have registered deployable "search-service"
    When I query the "deployable" graph with: { getByName(name: "search-service") { id name } }
    Then there should be no GraphQL errors
    And the response data should contain "search-service"

  Scenario: Temporal query returns deployable as it was
    Given I have registered deployable "versioned-service"
    And I capture the current timestamp as "before_update"
    And I update deployable "versioned-service" with body {"name": "versioned-service", "description": "v2 description"}
    When I query the "deployable" graph with: { getById(id: "<ids.versioned-service>", at: <timestamps.before_update>) { name description } }
    Then there should be no GraphQL errors
    And the response data description should be null
