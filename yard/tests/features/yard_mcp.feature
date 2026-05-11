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

  Scenario: tools/list returns the auto-derived catalogue + custom capabilities
    When I send MCP request "tools/list"
    Then the response should include tool "list_test_environments"
    And the response should include tool "list_test_runs"
    And the response should include tool "get_test_environment_by_id"
    And the response should include tool "history_for_environment"
    And the response should include tool "availability_for_environment"
    And the response should include tool "estimate_for_change_request"
    And the response should include tool "recommend_data_sync"

  Scenario: list_test_environments returns seeded test environments
    When I call MCP tool "list_test_environments" with arguments {}
    Then the tool result should be a JSON array of at least 4 records

  Scenario: list_test_runs returns seeded test runs
    When I call MCP tool "list_test_runs" with arguments {}
    Then the tool result should be a JSON array of at least 4 records

  Scenario: history_for_environment aggregates runs for a known environment
    When I call MCP tool "history_for_environment" with arguments {"test_environment_id": "<ids.edge-env>"}
    Then the tool result run_count should be at least 1

  Scenario: recommend_data_sync returns a push recommendation for event edges
    When I call MCP tool "recommend_data_sync" with arguments {"edge": "event"}
    Then the tool result kind should equal "push"
