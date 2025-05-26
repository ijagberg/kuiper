# kuiper

CLI tool for sending predefined HTTP requests.

## Usage

To run a request:

`kuiper path/to/request_file -e env_file.env`

## Directory structure

When you run a request, `kuiper` traverses the directories on the way to the specified file, and looks for `headers.json` files on the way. Header values in child directories take precedence over their parents. The request in the specified file can also have headers specified, which takes precedence over the files on the way to the request. Finally, you can specify additional header files with `-H`, which take precedence over everything else. Take a look at the `requests` folder in the source repository for this project for some examples.

```
parent/
| headers.json {"header_a": "value_a", "header_b": "value_b"}
| request.json
|   ^ this request would have {"header_a": "value_a", "header_b": "value_b"}
| child/
  | headers.json {"header_b": "value_c"}
  | request.json // request file in child dir
  | ^ this request would have {"header_a": "value_a", "header_b": "value_c"}
```

Headers can be removed by explicitly setting them to `null`, so if you wanted `child/request.json` to not send the `header_a` header, you could either set it to `null` in `request.json`, or in `child/headers.json`

## Request file format

Request files are just JSON files, and look like this:

```json
{
  "uri": "http://localhost/api/user/{{env:user_id}}",
  "method": "GET",
  "params": {
    "query_param_1": "query_param_value"
  },
  "headers": {
    "request_specific_header_1": "request_specific_header_value_1"
  }
}
```

## Interpolation

Values can be evaluated dynamically in two different ways when parsing a `.kuiper` file.

- `{{env:ENV_VAR}}`
  This will be replaced by the value of the environment variable `ENV_VAR`.
- `{{expr:EXPRESSION}}`
  This will be replaced by a value depending on what `EXPRESSION` is. Currently, the only supported expressions are `uuid`, for generating a uuid, and `now` for generating a timestamp.

# Background

I often find myself wanting to send simple HTTP requests when I am building APIs, and historically I have used Postman for this. But Postman is pretty bloated in my opinion - doing too much not well enough. `kuiper` is not intended to replace what Postman can do with automated integration tests, pre- and post-request scripts etc. It is just a tool for defining and running HTTP requests in a manner that can be source controlled.
