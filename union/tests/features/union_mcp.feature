Feature: Union MCP server

  Background:
    Given a Union server is running
    And the MCP binary is started against the server
    And I have registered team "Platform" of kind "platform"
    And I have registered team "Payments" of kind "product"
    And I have registered team "Security" of kind "security"
    And I have registered team "Data" of kind "domain"
    And I have registered team "Support" of kind "support"
    And I have registered person "Ada" with role "engineer"
    And I have registered person "Bea" with role "engineer"
    And I have placed "Ada" on team "Platform" as "lead"
    And I have placed "Bea" on team "Platform" as "engineer"
    And I have filed work order "ship-auth" for team "Platform" worth 5 points with status "in_progress"
    And I have filed work order "ship-pay" for team "Platform" worth 8 points with status "proposed"
    And I have filed work order "old-cleanup" for team "Platform" worth 13 points with status "done"

  Scenario: tools/list returns the catalogue + custom tools
    When I send MCP request "tools/list"
    Then the response should include tool "catalog.list"
    And the response should include tool "catalog.get"
    And the response should include tool "catalog.search"
    And the response should include tool "team.capacity"
    And the response should include tool "team.members"
    And the response should include tool "person.assignments"

  Scenario: catalog.list returns teams
    When I call MCP tool "catalog.list" with arguments {"entity": "team"}
    Then the tool result should be a JSON array of at least 5 records

  Scenario: team.capacity sums open story points
    When I call MCP tool "team.capacity" with arguments {"team_id": "<ids.Platform>"}
    Then the tool result should report points_in_flight 13
    And the tool result should report open_work_order_count 2
    And the tool result should report member_count 2
