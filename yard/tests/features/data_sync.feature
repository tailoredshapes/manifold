Feature: Data sync between test environments

  Background:
    Given a Yard server is running
    And I have registered test_environment "checkout-iso" with kind "isolated"
    And I have registered test_environment "auth-iso" with kind "isolated"

  Scenario: Open a pull sync from auth-iso to checkout-iso
    When I POST to "/data_sync/api" with body {"kind": "pull", "target_env_id": "<ids.checkout-iso>", "source_env_id": "<ids.auth-iso>", "estimated_minutes": "45"}
    Then the response status should be 201
    And the response body should contain "pull"

  Scenario: Cannot open a sync without a source
    When I POST to "/data_sync/api" with body {"kind": "pull", "target_env_id": "<ids.checkout-iso>"}
    Then the response status should be 400
    And the response body should contain "source_env_id"

  Scenario: Cannot open a shared sync against a static data source
    Given I have registered data_source "fixture-bag" with kind "fixtures"
    When I POST to "/data_sync/api" with body {"kind": "shared", "target_env_id": "<ids.checkout-iso>", "source_data_id": "<ids.fixture-bag>"}
    Then the response status should be 400

  Scenario: Cannot open a sync with an unknown kind
    When I POST to "/data_sync/api" with body {"kind": "magic", "target_env_id": "<ids.checkout-iso>", "source_env_id": "<ids.auth-iso>"}
    Then the response status should be 400

  Scenario: Recommend push for an event-based dependency
    When I POST to "/data_sync/recommend" with body {"edge": "event-based"}
    Then the response status should be 200
    And the response body should contain "push"

  Scenario: Recommend pull for an API dependency
    When I POST to "/data_sync/recommend" with body {"edge": "api"}
    Then the response status should be 200
    And the response body should contain "pull"

  Scenario: Recommend shared for a shared-DB dependency
    When I POST to "/data_sync/recommend" with body {"edge": "shared-db"}
    Then the response status should be 200
    And the response body should contain "shared"

  Scenario: Reject a recommendation request with an unknown edge
    When I POST to "/data_sync/recommend" with body {"edge": "smoke-signals"}
    Then the response status should be 400
