# JSON Streams Flat API

The JSON Streams HTTP endpoint allows aggregate instances to be queried with a MongoDB aggregation pipeline. It may not be convenient to put this in a UI. This endpoint can be put in front of it to hide the pipelines. You can create a URI space, where each path corresponds to an aggregation pipeline on some aggregate collection. You post a flat JSON document to it that is merged into the underlying aggregation pipeline through templating.

## Configuration

The configuration for the flat API supports the fields that are described in the [`salvo-from-config`](https://docs.rs/salvo-from-config/latest/salvo_from_config/) crate. Then there are the following fields:

|Field|Description|
|---|---|
|context&#x2011;path|The optional context path for the URI space.|
|handlers|The array of path handlers.|
|handlers.aggregation|The aggregation pipeline template. If this is a YAML object, then substitutions are limited to references to path parameters and dot-separated fields in the posted JSON object. Path parameter references have the form `$$name`, while field references have the form `${name}`. If the `aggregation` field is a string field, then you can use the [jinja2](https://jinja.palletsprojects.com/en/stable/templates/) syntax to construct the aggregation pipeline. For the global variable references, the path parameters and the fields in the posted JSON object form one space. Dot-separated references are supported. Set the log level to `debug` to see what is generated and forwarded.|
|handlers.json&#x2011;streams&#x2011;path|The path at the JSON Streams endpoint to which the aggregation pipelines are forwarded.|
|handlers.path|The path that is exposed in the flat API. It can have parameters in the form `{name}`.|
|json&#x2011;streams&#x2011;endpoint|The URL to which the generated aggregation pipelines are forwarded.|

This is an example:

```yaml
context-path: /flat
log-level: debug
json-streams-endpoint: http://jes-http.json-streams:9000
handlers:
  - path: /template-test
    json-streams-path: /api/app/aggregate
    aggregation: |
      - $match:
      {% if myField %}
          myField:
            $in:
              - {{ myField }}
      {% endif %}
          otherField: {{ someField.subfield }}
```

## Running It

The CLI has only an option to set the port, the default value of which is 8080.

```
> json-streams-flat --port 8888
```

## Docker

Docker images can be found at [https://hub.docker.com/repository/docker/wdonne/json-streams-flat](https://hub.docker.com/repository/docker/wdonne/json-streams-flat).

## Kubernetes

Install the service as follows:

```bash
helm repo add wdonne https://wdonne.github.io/helm
helm repo update
helm install mongo-jwt-claims wdonne/json-streams-flat --namespace json-streams-flat --create-namespace
```