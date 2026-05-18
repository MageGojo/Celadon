; Separator (### ...)
(separator) @comment

; Comments (# ...)
(comment) @comment

; HTTP methods: GET, POST, PUT, DELETE ...
(method) @keyword.return

; URL
(url) @string

; HTTP version (HTTP/1.1, HTTP/2)
(http_version) @keyword.return

; Response status code
(status_code) @number

; Response reason phrase
(reason_phrase) @string

; Header name
(header_name) @type

; Header value
(header_value) @string.special

