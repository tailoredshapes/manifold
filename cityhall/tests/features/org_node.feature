Feature: OrgNode hierarchy

  Background:
    Given a Cityhall server is running

  Scenario: Build a four-level hierarchy
    Given I have built the standard hierarchy
    When I GET "/org_node/<ids.checkout>/ancestors"
    Then the response status should be 200
    And the response body should contain "Acme"
    And the response body should contain "Engineering"
    And the response body should contain "Payments"
    And the response body should contain "Checkout Team"

  Scenario: Enterprise must not have a parent
    When I POST to "/org_node/api" with body {"name": "FloatingEnterprise", "kind": "enterprise", "parent_id": "anywhere"}
    Then the response status should be 400

  Scenario: Non-enterprise must have a parent
    When I POST to "/org_node/api" with body {"name": "FloatingDomain", "kind": "domain"}
    Then the response status should be 400

  Scenario: Reject unknown kind
    When I POST to "/org_node/api" with body {"name": "Mystery", "kind": "alien"}
    Then the response status should be 400

  Scenario: Find children of a node via GraphQL
    Given I have built the standard hierarchy
    When I query the "org_node" graph with: { getByParentId(parent_id: "<ids.payments>") { name kind } }
    Then there should be no GraphQL errors
    And the response data should contain "Checkout Team"
    And the response data should contain "Auth Team"
