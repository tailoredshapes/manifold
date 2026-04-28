Feature: Web UI static assets

  Background:
    Given a Groundwork server is running

  Scenario: Root path serves the index page
    When I GET "/"
    Then the response status should be 200
    And the response content-type should contain "text/html"
    And the response body should contain "<title>groundwork</title>"
    And the response body should contain "app.js"

  Scenario: App JS is served
    When I GET "/static/app.js"
    Then the response status should be 200
    And the response content-type should contain "javascript"
    And the response body should contain "fetchApps"
    And the response body should contain "registerApp"

  Scenario: Health check endpoint
    When I GET "/health"
    Then the response status should be 200
    And the response body should contain "ok"

  Scenario: Unknown routes return 404
    When I GET "/does-not-exist"
    Then the response status should be 404
