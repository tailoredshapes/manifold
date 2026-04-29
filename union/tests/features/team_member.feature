Feature: Team membership

  Background:
    Given a Union server is running
    And I have registered person "Ada Lovelace"
    And I have registered team "checkout-team" with kind "product"

  Scenario: Assign a person to a team
    When I POST to "/team_member/api" with body {"person_id": "<ids.Ada Lovelace>", "team_id": "<ids.checkout-team>", "role": "lead"}
    Then the response status should be 201
    And the response body should have an "id" field

  Scenario: Cannot assign without person_id
    When I POST to "/team_member/api" with body {"team_id": "<ids.checkout-team>"}
    Then the response status should be 400

  Scenario: Cannot assign without team_id
    When I POST to "/team_member/api" with body {"person_id": "<ids.Ada Lovelace>"}
    Then the response status should be 400

  Scenario: List members of a team via GraphQL
    Given I have registered person "Grace Hopper"
    When I POST to "/team_member/api" with body {"person_id": "<ids.Ada Lovelace>", "team_id": "<ids.checkout-team>"}
    And I POST to "/team_member/api" with body {"person_id": "<ids.Grace Hopper>", "team_id": "<ids.checkout-team>"}
    And I query the "team_member" graph with: { getByTeamId(team_id: "<ids.checkout-team>") { person_id role } }
    Then there should be no GraphQL errors
    And the response data should contain "<ids.Ada Lovelace>"
    And the response data should contain "<ids.Grace Hopper>"

  Scenario: A person can be on multiple teams (matrix)
    Given I have registered team "appsec" with kind "security"
    When I POST to "/team_member/api" with body {"person_id": "<ids.Ada Lovelace>", "team_id": "<ids.checkout-team>", "role": "lead"}
    And I POST to "/team_member/api" with body {"person_id": "<ids.Ada Lovelace>", "team_id": "<ids.appsec>", "role": "champion"}
    And I query the "team_member" graph with: { getByPersonId(person_id: "<ids.Ada Lovelace>") { team_id role } }
    Then there should be no GraphQL errors
    And the response data should contain "<ids.checkout-team>"
    And the response data should contain "<ids.appsec>"
