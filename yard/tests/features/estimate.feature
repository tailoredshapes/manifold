Feature: Estimate infrastructure / data / coordination tasks for a Cityhall ChangeRequest

  Background:
    Given a Yard server is running
    And the Groundwork stub knows deployable "dep-checkout" as "checkout" depending on "dep-auth"
    And the Groundwork stub knows deployable "dep-auth" as "auth" with no dependencies
    And the Cityhall stub knows change request "cr-1" with summary "deploy-v2" targeting "dep-checkout"
    And I have registered test_environment "checkout-iso" with kind "isolated" for deployable "dep-checkout" with spinup_minutes "20" and cost_per_hour "0.10"
    And I have registered test_environment "auth-iso" with kind "isolated" for deployable "dep-auth" with spinup_minutes "10" and cost_per_hour "0.05"

  Scenario: Estimate emits an infrastructure task per deployable in topo order
    When I POST to "/change_request/cr-1/estimate" with body {"tier": "dev"}
    Then the response status should be 200
    And the response body should contain "infrastructure"
    And the response body should contain "spin up auth-iso"
    And the response body should contain "spin up checkout-iso"

  Scenario: Estimate emits a data-sync task per Groundwork dep edge
    When I POST to "/change_request/cr-1/estimate" with body {"tier": "dev"}
    Then the response status should be 200
    And the response body should contain "sync auth-iso → checkout-iso"

  Scenario: Estimate flags missing test envs as blockers
    Given the Cityhall stub knows change request "cr-orphan" with summary "ghost" targeting "dep-ghost"
    When I POST to "/change_request/cr-orphan/estimate" with body {"tier": "dev"}
    Then the response status should be 200
    And the response body should contain "unknown deployable: dep-ghost"

  Scenario: Estimate sums total minutes and cost
    When I POST to "/change_request/cr-1/estimate" with body {"tier": "dev"}
    Then the response status should be 200
    And the estimate total_minutes should be at least 30
    And the estimate total_cost should be greater than 0

  Scenario: Estimate emits a coordination wait for a rate-limited external env
    Given I have registered test_environment "stripe-sandbox" with kind "external" for deployable "dep-checkout" with spinup_minutes "5" and contractual_limit "5" and rate_limit "10/min"
    When I POST to "/change_request/cr-1/estimate" with body {"tier": "dev"}
    Then the response status should be 200
    And the response body should contain "coordination"
