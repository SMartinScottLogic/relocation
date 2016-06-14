var EventEmitter = require('events').EventEmitter
var args = require('cli.args')()
var fs = require('fs');
var path = require('path')

var emitter = new EventEmitter();

function get_traits( stat ) {
    return ['File','Directory','BlockDevice','CharacterDevice','SymbolicLink','FIFO','Socket']
    .reduce( (traits, trait) => (traits[trait] = stat[`is${trait}`](),traits), {})
}

var jobs = 0
var ended = false
function handle_single_path(root, pathname) {
    fs.readdir(pathname, (err, entries) => {
        jobs++
        if (err) {
            jobs--
            return emitter.emit('error', err, pathname)
        }
        entries.forEach((entry) => {
            jobs++
            const fullpathname = path.join(pathname, entry)
            fs.lstat(fullpathname, (err, stat) => {
                if (err) {
                    jobs--
                    return emitter.emit('error', err, fullpathname)
                }
                emitter.emit('file', null, {pathname: fullpathname, stat, traits: get_traits(stat)})
                if(stat.isDirectory()) {
                    emitter.emit('path', null, fullpathname, root)
                }
                jobs--
            })
        })
        jobs--
        process.nextTick(function() {
            if(jobs <= 0 && !ended) {
                ended = 1
                emitter.emit('end', null)
            }
        })
    })
}

args.nonOpt.map((filename) => handle_single_path(filename, filename))
emitter.on('error', (err, pathname) => {
    console.log('ERROR', new Date(), err, pathname)
})
emitter.on('file', (err, file) => {
    //console.log(new Date(), file.pathname, 'file', file.traits.File)
})
emitter.on('path', (err, path, root) => {
    setTimeout( () => {
        handle_single_path( root, path)
    }, 1000)
})
emitter.on('end', (err) => {
    console.log('done')
})