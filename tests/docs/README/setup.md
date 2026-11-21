<!-- The suite the README's tree shows: clean, with one transition, and built. -->

```console
$ missouri init
initialized missouri project at [CWD]/.missouri

$ missouri state add clean
created state "clean"

$ missouri state add built
created state "built"

```

```yaml {filename="clean/.missouri/missouri.yml"}
transitions: [{command: "echo hello > out.txt", target: ../built}]
```

```text {filename="built/out.txt"}
hello
```

