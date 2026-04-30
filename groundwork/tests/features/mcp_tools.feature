Feature: Groundwork MCP server

  Background:
    Given a Groundwork server is running
    And the MCP binary is started against the server
    And I have registered deployable "checkout"
    And I have registered service "payments-api"
    And I have recorded that "checkout" depends on "payments-api"

  Scenario: tools/list returns the catalogue
    When I send MCP request "tools/list"
    Then the response should include tool "catalog.list"
    And the response should include tool "catalog.get"
    And the response should include tool "catalog.search"
    And the response should include tool "graph.blast_radius"
    And the response should include tool "graph.dependencies_of"
    And the response should include tool "graph.deployment_plan"

  Scenario: catalog.list returns deployables
    When I call MCP tool "catalog.list" with arguments {"entity": "deployable"}
    Then the tool result should be a JSON array
    And the tool result should contain a record named "checkout"

  Scenario: catalog.get fetches a single deployable
    When I call MCP tool "catalog.get" with arguments {"entity": "deployable", "id": "<ids.checkout>"}
    Then the tool result envelope name should be "checkout"

  Scenario: catalog.search filters by name substring
    When I call MCP tool "catalog.search" with arguments {"entity": "service", "name": "payments"}
    Then the tool result should be a JSON array
    And the tool result should contain a record named "payments-api"

  Scenario: graph.blast_radius returns dependents
    When I call MCP tool "graph.blast_radius" with arguments {"service_id": "<ids.payments-api>"}
    Then the tool result should describe "checkout" as a dependent

  Scenario: graph.deployment_plan orders dependencies first
    Given I have registered service "auth-api"
    And I have registered deployable "auth"
    And I have recorded that "auth" exposes "auth-api"
    And I have recorded that "checkout" depends on "auth-api"
    When I call MCP tool "graph.deployment_plan" with arguments {"deployable_id": "<ids.checkout>"}
    Then the deployment plan should list "auth" before "checkout"

  Scenario: graph.deployment_plan flags managed external services
    When I call MCP tool "graph.deployment_plan" with arguments {"deployable_id": "<ids.checkout>"}
    Then the deployment plan should list "payments-api" as an external prerequisite

  Scenario: graph.dependencies_of walks forward
    Given I have registered service "auth-api"
    And I have registered deployable "auth"
    And I have recorded that "auth" exposes "auth-api"
    And I have recorded that "checkout" depends on "auth-api"
    When I call MCP tool "graph.dependencies_of" with arguments {"deployable_id": "<ids.checkout>"}
    Then the tool result should describe "auth" as a dependency
