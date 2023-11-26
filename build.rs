fn main() {
    if cfg!(target_os = "windows") {
        let mut res = winres::WindowsResource::new();
        res.add_toolkit_include(true);
        res.append_rc_content(
            r#"
#include "winres.h"
101 PNG "dvd.png""#,
        );
        res.write_resource_file("resource.rc").unwrap();
        res.compile().unwrap();
    }
}
