Feature: Test infrastructure

  Background:
    Given a Yard server is running

  Scenario: Register a docker host
    When I POST to "/test_infrastructure/api" with body {"name": "local-docker", "provider": "docker"}
    Then the response status should be 201
    And the response body should contain "local-docker"

  Scenario: Cannot register with an unknown provider
    When I POST to "/test_infrastructure/api" with body {"name": "x", "provider": "magic"}
    Then the response status should be 400

  Scenario: Cannot register without a provider
    When I POST to "/test_infrastructure/api" with body {"name": "naked"}
    Then the response status should be 400

  Scenario: Find infrastructure by provider
    Given I have registered test_infrastructure "ec2-1" with provider "aws_ec2"
    And I have registered test_infrastructure "k8s-1" with provider "kubernetes"
    When I query the "test_infrastructure" graph with: { getByProvider(provider: "aws_ec2") { name } }
    Then there should be no GraphQL errors
    And the response data should contain "ec2-1"
    And the response data should not contain "k8s-1"
