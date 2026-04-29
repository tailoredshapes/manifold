Feature: Person CRUD

  Background:
    Given a Union server is running

  Scenario: Register a person with just a name
    When I POST to "/person/api" with body {"name": "Ada Lovelace"}
    Then the response status should be 201
    And the response body should have an "id" field
    And the response body should contain "Ada Lovelace"

  Scenario: Cannot register a person without a name
    When I POST to "/person/api" with body {}
    Then the response status should be 400

  Scenario: Update contact and role
    Given I have registered person "Grace Hopper"
    When I PUT "/person/api/<ids.Grace Hopper>" with body {"name": "Grace Hopper", "contact": "ghopper@navy.mil", "role": "Rear Admiral"}
    Then the response status should be 200
    And the response body should contain "Rear Admiral"

  Scenario: Find by name via GraphQL
    Given I have registered person "Margaret Hamilton"
    When I query the "person" graph with: { getByName(name: "Margaret Hamilton") { id name } }
    Then there should be no GraphQL errors
    And the response data should contain "Margaret Hamilton"

  Scenario: Delete a person
    Given I have registered person "Temp Person"
    When I DELETE "/person/api/<ids.Temp Person>"
    Then the response status should be 200
    When I GET "/person/api/<ids.Temp Person>"
    Then the response status should be 404
