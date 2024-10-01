# kuiper

CLI tool for sending predefined HTTP requests.

## Usage

To run a request:

`kuiper path/to/request.kuiper -e env_file.env`

## Directory structure

When you run a request, `kuiper` traverses the directories on the way to the `.kuiper` file, and looks for `headers.json` files on the way. Header values in child directories take precedence over their parents. The request in the `.kuiper` file can also have headers specified, which takes precedence over everything else. Take a look at the `requests` folder in the source repository for this project for an example.

```
parent/
| headers.json {"header_a": "value_a", "header_b": "value_b"}
| request.kuiper
|   ^ this request would have {"header_a": "value_a", "header_b": "value_b"}
| child/
  | headers.json {"header_b": "value_c"}
  | request.kuiper // request file in child dir
  | ^ this request would have {"header_a": "value_a", "header_b": "value_c"}
```

Headers can be removed by explicitly setting them to `null`.

## .kuiper format

`.kuiper` files are just JSON files, and look like this:

```json
{
  "uri": "http://localhost/api/user/1",
  "method": "GET",
  "headers": {
    "request_specific_header_1": "request_specific_header_value_1"
  }
}
```

# Background

I often find myself wanting to send simple HTTP requests when I am building APIs, and historically I have used Postman for this. But Postman is a pretty bloated piece of software that does too much, and too poorly. `kuiper` is not intended to replace what Postman can do with automated integration tests, pre- and post-request scripts etc. It is just a tool for defining and running HTTP requests in a manner that can be source controlled.
