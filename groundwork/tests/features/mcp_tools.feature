Feature: Groundwork MCP server

  Background:
    Given a Groundwork server is running
    And the MCP binary is started against the server
    And I have registered deployable "checkout"
    And I have registered service "payments-api"
    And I have recorded that "checkout" depends on "payments-api"

  Scenario: tools/list returns the auto-derived catalogue and custom capabilities
    When I send MCP request "tools/list"
    Then the response should include tool "list_deployables"
    And the response should include tool "get_deployable_by_id"
    And the response should include tool "find_deployables_by_name"
    And the response should include tool "list_services"
    And the response should include tool "blast_radius_for_service"
    And the response should include tool "dependencies_of_deployable"
    And the response should include tool "deployment_plan_for_deployable"

  Scenario: list_deployables returns flat GraphQL rows
    When I call MCP tool "list_deployables" with arguments {}
    Then the tool result should be a JSON array
    And the tool result should contain a record named "checkout"

  Scenario: get_deployable_by_id fetches a single deployable
    When I call MCP tool "get_deployable_by_id" with arguments {"id": "<ids.checkout>"}
    Then the tool result name should be "checkout"

  Scenario: find_services_by_name filters by name
    When I call MCP tool "find_services_by_name" with arguments {"name": "payments-api"}
    Then the tool result should be a JSON array
    And the tool result should contain a record named "payments-api"

  Scenario: blast_radius_for_service returns dependents
    When I call MCP tool "blast_radius_for_service" with arguments {"service_id": "<ids.payments-api>"}
    Then the tool result should describe "checkout" as a dependent

  Scenario: deployment_plan_for_deployable orders dependencies first
    Given I have registered service "auth-api"
    And I have registered deployable "auth"
    And I have recorded that "auth" exposes "auth-api"
    And I have recorded that "checkout" depends on "auth-api"
    When I call MCP tool "deployment_plan_for_deployable" with arguments {"deployable_id": "<ids.checkout>"}
    Then the deployment plan should list "auth" before "checkout"

  Scenario: deployment_plan_for_deployable flags managed external services
    When I call MCP tool "deployment_plan_for_deployable" with arguments {"deployable_id": "<ids.checkout>"}
    Then the deployment plan should list "payments-api" as an external prerequisite

  Scenario: dependencies_of_deployable walks forward
    Given I have registered service "auth-api"
    And I have registered deployable "auth"
    And I have recorded that "auth" exposes "auth-api"
    And I have recorded that "checkout" depends on "auth-api"
    When I call MCP tool "dependencies_of_deployable" with arguments {"deployable_id": "<ids.checkout>"}
    Then the tool result should describe "auth" as a dependency
