Feature: Mermaid Gantt output from deployment plan

  Background:
    Given a Cityhall server is running
    And I have computed a deployment plan with 2 sequential steps and one ApprovalGate

  Scenario: Generate Gantt for the plan
    When I POST to "/deployment_plan/<ids.plan>/gantt" with body {}
    Then the response status should be 201
    And the response body should contain "gantt"
    And the response body should contain "title"
    And the response body should contain "dateFormat"
    And the response body should contain "section"

  Scenario: Each step appears as a Gantt task
    When I POST to "/deployment_plan/<ids.plan>/gantt" with body {}
    Then the response body should contain "step_0"
    And the response body should contain "step_1"

  Scenario: ApprovalGate appears as a milestone
    When I POST to "/deployment_plan/<ids.plan>/gantt" with body {}
    Then the response body should contain ":crit, milestone"
    And the response body should contain "ApprovalGate"

  Scenario: Sections group steps by deployable
    When I POST to "/deployment_plan/<ids.plan>/gantt" with body {}
    Then the response body should contain "section auth"
    And the response body should contain "section checkout"

  Scenario: Deterministic output across two calls
    When I POST to "/deployment_plan/<ids.plan>/gantt" with body {}
    And I POST to "/deployment_plan/<ids.plan>/gantt" with body {}
    Then both responses should be byte-equal
