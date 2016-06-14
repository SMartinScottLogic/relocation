'use strict'

var walk = require('walkdir')
var args = require('cli.args')()
var path = require('path')

function update_stats(root, filename, stat) {
    console.log('file', root, filename, path.relative(root, filename))
    const paths = path.relative(root, filename).split(path.sep).reduce( (prev, cur) => {
        console.log('debug', prev, cur)
        if(prev.length == 0 ) {
            prev.push(cur)
        } else {
            const tail = prev.slice(-1)[0]
            prev.push( path.join(tail, cur) )
        }
        return prev
    }, [])
    console.log(paths);
    if(paths.length > 3) process.exit(-1);
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