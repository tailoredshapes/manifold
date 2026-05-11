Feature: Deployment plan computed from a change request

  Background:
    Given a Cityhall server is running
    And I have built the standard hierarchy

  Scenario: Plan with no bylaws — direct deploy
    Given I have a change request "deploy-checkout-v2" with target deployables ["dep-checkout"]
    When I POST to "/change_request/<ids.deploy-checkout-v2>/plan" with body {"tier": "dev"}
    Then the response status should be 201
    And the response body should contain "change_request_id"
    And the plan should have 2 steps
    And the plan step 0 should be "deploy auth"
    And the plan step 1 should be "deploy checkout"

  Scenario: Plan with an enterprise FreezePeriod adds a gate to every step
    Given enterprise "<ids.acme>" has a "FreezePeriod" bylaw with window "2026-12-23T00:00Z/2026-12-27T00:00Z"
    And I have a change request "deploy-checkout-v3" with target deployables ["dep-checkout"]
    When I POST to "/change_request/<ids.deploy-checkout-v3>/plan" with body {"tier": "prod"}
    Then the response status should be 201
    And the plan step 0 should have a "FreezePeriod" gate
    And the plan step 1 should have a "FreezePeriod" gate

  Scenario: Plan flags an orphan deployable as a blocker
    Given I have a change request "deploy-orphan" with target deployables ["dep-orphan"]
    When I POST to "/change_request/<ids.deploy-orphan>/plan" with body {"tier": "prod"}
    Then the response status should be 201
    And the plan blockers should contain "orphan: orphan"
