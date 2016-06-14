'use strict'

var walk = require('walkdir')
var args = require('cli.args')()
var path = require('path')

let space = {}
function update_stats(root, filename, stat) {
    const size = stat.size

    console.log('file', root, filename, size)

    let done = false
    const parsed = path.parse(filename)
    path.relative(root, parsed.dir).split(path.sep).reduce((prev, cur) => {
        if (prev.length === 0) {
            prev.push('.')
        }
        if(cur.length === 0) return prev
        const tail = prev.slice(-1)[0]
        prev.push(path.join(tail, cur))
        if (prev.length > 2) done = true
        return prev
    }, []).forEach((name) => (console.log(name), space[name] = (space[name] || 0) + size))

    if (done) {
        console.log(require('util').inspect(space, { depth: null }))
        process.exit(-1)
    }
}

function handle_single_path(root, pathname) {
    let files = 0
    return new Promise((resolve, reject) => {
        var emitter = walk(pathname);

        emitter.on('file', function onFile(filename, stat) {
            //console.log('file from emitter: ', filename);
            update_stats(root, filename, stat)
            files++
        });
        emitter.on('error', function onError(path, err) {
            console.log('ERROR', new Date(), err, path)
        })
        emitter.on('fail', function onFail(path, err) {
            console.log('FAIL', new Date(), path.length, err, path)
        })
        emitter.on('end', function onDone() {
            console.log('done', new Date(), `[found ${files} files]`)
            resolve(true);
        })
    })
}

args.nonOpt.map((filename) => handle_single_path(filename, filename))