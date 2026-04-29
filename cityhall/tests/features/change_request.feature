Feature: Change requests

  Background:
    Given a Cityhall server is running

  Scenario: Submit a minimal change request
    When I POST to "/change_request/api" with body {"summary": "bump checkout to v1.2.3"}
    Then the response status should be 201
    And the response body should have an "id" field

  Scenario: Reject invalid status
    When I POST to "/change_request/api" with body {"summary": "x", "status": "yolo"}
    Then the response status should be 400

  Scenario: Reject invalid tier
    When I POST to "/change_request/api" with body {"summary": "x", "tier": "moon"}
    Then the response status should be 400

  Scenario: Status moves through the workflow
    Given I have submitted change request "bump-checkout"
    When I PUT "/change_request/api/<ids.bump-checkout>" with body {"summary": "bump-checkout", "status": "submitted"}
    Then the response status should be 200
    And the response body should contain "submitted"

  Scenario: Find change requests by status
    Given I have submitted change request "in-flight"
    And I have submitted change request "later"
    When I PUT "/change_request/api/<ids.in-flight>" with body {"summary": "in-flight", "status": "submitted"}
    And I query the "change_request" graph with: { getByStatus(status: "submitted") { summary } }
    Then there should be no GraphQL errors
    And the response data should contain "in-flight"
