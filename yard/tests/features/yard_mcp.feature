Feature: Yard MCP server

  Background:
    Given a Yard server is running
    And the MCP binary is started against the server
    And I have registered test environment "edge-env" of kind "isolated"
    And I have registered test environment "smoke-env" of kind "sandbox"
    And I have registered test environment "soak-env" of kind "multi-tenant"
    And I have registered test environment "prod-mirror" of kind "external"
    And I have logged test run for "edge-env" with status "passed" lasting 12 minutes
    And I have logged test run for "edge-env" with status "passed" lasting 9 minutes
    And I have logged test run for "edge-env" with status "failed" lasting 7 minutes
    And I have logged test run for "smoke-env" with status "passed" lasting 4 minutes

  Scenario: tools/list returns the catalogue + custom tools
    When I send MCP request "tools/list"
    Then the response should include tool "catalog.list"
    And the response should include tool "catalog.get"
    And the response should include tool "catalog.search"
    And the response should include tool "environment.history"
    And the response should include tool "environment.availability"
    And the response should include tool "change_request.estimate"
    And the response should include tool "data_sync.recommend"

  Scenario: catalog.list returns seeded test environments
    When I call MCP tool "catalog.list" with arguments {"entity": "test_environment"}
    Then the tool result should be a JSON array of at least 4 records

  Scenario: catalog.list returns seeded test runs
    When I call MCP tool "catalog.list" with arguments {"entity": "test_run"}
    Then the tool result should be a JSON array of at least 4 records

  Scenario: environment.history aggregates runs for a known environment
    When I call MCP tool "environment.history" with arguments {"test_environment_id": "<ids.edge-env>"}
    Then the tool result run_count should be at least 1

  Scenario: data_sync.recommend returns a push recommendation for event edges
    When I call MCP tool "data_sync.recommend" with arguments {"edge": "event"}
    Then the tool result kind should equal "push"
