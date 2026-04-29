Feature: Mock sources

  Background:
    Given a Yard server is running

  Scenario: Register a mock source
    When I POST to "/mock_source/api" with body {"name": "auth-mock", "repo_url": "git@example.com:mocks/auth.git", "path": "fixtures/auth", "language": "go"}
    Then the response status should be 201
    And the response body should contain "auth-mock"

  Scenario: Cannot register without a name
    When I POST to "/mock_source/api" with body {"repo_url": "git@example.com:mocks/x.git"}
    Then the response status should be 400

  Scenario: Find mock sources by language
    Given I have registered mock_source "go-mocks" with language "go"
    And I have registered mock_source "rust-mocks" with language "rust"
    When I query the "mock_source" graph with: { getByLanguage(language: "go") { name } }
    Then there should be no GraphQL errors
    And the response data should contain "go-mocks"
    And the response data should not contain "rust-mocks"
